use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::common::{Id, Node};
use crate::messages::{
    FindNodeRequestArguments, FindNodeResponseArguments, Message, MessageType,
    PingRequestArguments, PingResponseArguments, RequestSpecific, ResponseSpecific,
};

use crate::query::Query;
use crate::routing_table::RoutingTable;
use crate::socket::KrpcSocket;
use crate::Result;

const DEFAULT_PORT: u16 = 6881;
const DEFAULT_TIMEOUT_MILLIS: u64 = 500;
const VERSION: &[u8] = "RS".as_bytes(); // The Mainline rust implementation.
const MTU: usize = 2048;
const TICK_INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_BOOTSTRAP_NODES: [&str; 9] = [
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401", // @arvidn's
    "router.bittorrent.com:6881",
    "router.pkarr.org:6881",
    "router.bittorrent.cloud:42069", // Seems to be read-only.
    "router.utorrent.com:6881",
    "dht.aelitis.com:6881",   // Vuze doesn't respond in home network.
    "router.silotis.us:6881", // IPv6
    "dht.anacrolix.link:42069",
];

#[derive(Debug)]
pub struct Rpc {
    id: Id,
    socket: KrpcSocket,
    bootstrap: Vec<String>,
    routing_table: RoutingTable,
    inflight_queries: BTreeMap<Id, Query>,
}

impl Rpc {
    pub fn new() -> Result<Self> {
        // TODO: One day I might implement BEP42.
        let id = Id::random();

        let socket = KrpcSocket::new()?;

        Ok(Rpc {
            id,
            socket,
            bootstrap: DEFAULT_BOOTSTRAP_NODES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            routing_table: RoutingTable::new().with_id(id),
            inflight_queries: BTreeMap::new(),
        })
    }

    // === Options ===

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.socket.read_only = read_only;
        self
    }

    /// Sets requests timeout in milliseconds
    pub fn with_request_timout(mut self, timeout: u64) -> Self {
        self.socket.request_timeout = Duration::from_millis(timeout);
        self
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr()
    }

    // === Public Methods ===

    pub fn tick(&mut self) -> Option<(Message, SocketAddr)> {
        if let Some((message, from)) = self.socket.recv_from() {
            return self.handle_incoming_message(message, from);
        };

        // === Advance queries ===
        // for query in self.inflight_queries.values() {
        //     for node in query.closest.iter() {
        //         self.socket.request(node.address, &query.request);
        //     }
        // }

        thread::sleep(TICK_INTERVAL);
        None
    }

    pub fn ping(&mut self, address: SocketAddr) {
        self.socket.request(
            address,
            RequestSpecific::PingRequest(PingRequestArguments {
                requester_id: self.id,
            }),
        );
    }

    pub fn find_node(&mut self, address: SocketAddr, target: Id) {
        self.socket.request(
            address,
            RequestSpecific::FindNodeRequest(FindNodeRequestArguments {
                target,
                requester_id: self.id,
            }),
        );
    }

    /// Send a message to closer and closer nodes until we can't find any more nodes.
    pub fn query(&mut self, target: Id, request: RequestSpecific) {
        if let Some(query) = self.inflight_queries.get(&target) {
            // TODO: Update query (switch done to false?)
            return;
        }

        println!("Querying {:?}", target);
        let closest = match self.routing_table.is_empty() {
            true => {
                println!("Bootstrapping...");
                let mut result: Vec<Node> = vec![];

                for bootstrapping_node in self.bootstrap.clone() {
                    if let Ok(addresses) = bootstrapping_node.to_socket_addrs() {
                        for address in addresses {
                            if address.is_ipv6() {
                                // TODO: Add support for IPV6.
                                continue;
                            }
                            result.push(Node {
                                id: Id::random(),
                                address,
                            });
                        }
                    }
                }

                result
            }
            false => self.routing_table.closest(&target),
        };

        let mut query = Query::new(target, request, closest);

        self.inflight_queries.insert(target, query);
    }

    /// Ping bootstrap nodes, add them to the routing table with closest query.
    pub fn populate(&mut self) {
        self.query(
            self.id,
            RequestSpecific::FindNodeRequest(FindNodeRequestArguments {
                target: self.id,
                requester_id: self.id,
            }),
        );
    }

    // === Private Methods ===

    fn handle_incoming_message(
        &mut self,
        message: Message,
        from: SocketAddr,
    ) -> Option<(Message, SocketAddr)> {
        match &message.message_type {
            MessageType::Request(request_specific) => {
                return self.handle_request(from, message.transaction_id, request_specific)
            }
            MessageType::Response(response_specific) => {
                println!("Got response from {:?}:\n === {:?}\n", &from, &message);

                match response_specific {
                    ResponseSpecific::PingResponse(PingResponseArguments { responder_id }) => {
                        self.routing_table.add(Node {
                            id: *responder_id,
                            address: from,
                        });
                    }
                    ResponseSpecific::FindNodeResponse(FindNodeResponseArguments {
                        responder_id,
                        nodes,
                    }) => {
                        self.routing_table.add(Node {
                            id: *responder_id,
                            address: from,
                        });
                    }
                    _ => {}
                }
                return Some((message, from));
            }
            MessageType::Error(_) => return Some((message, from)),
        }
        // TODO: should we update the table?

        None
    }

    fn handle_request(
        &mut self,
        from: SocketAddr,
        transaction_id: u16,
        request: &RequestSpecific,
    ) -> Option<(Message, SocketAddr)> {
        match request {
            // TODO: Handle bad requests (send an error message).
            RequestSpecific::PingRequest(_) => {
                self.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::PingResponse(PingResponseArguments {
                        responder_id: self.id,
                    }),
                );
            }
            RequestSpecific::FindNodeRequest(_) => {
                self.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::FindNodeResponse(FindNodeResponseArguments {
                        responder_id: self.id,
                        nodes: self.routing_table.closest(&self.id),
                    }),
                );
            }
            _ => {
                // TODO: Handle queries (stuff with closer nodes in the response).
                // TODO: How to deal with unknown requests?
                // TODO: Send error message?
                // TODO: should we rsepond with FindNodeResponse anyways?
                todo!()
            }
        }

        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ping_response() {
        let mut server = Rpc::new().unwrap();
        let server_address = server.local_addr();
        let server_id = server.id;

        thread::spawn(move || loop {
            server.tick();
        });

        let mut client = Rpc::new().unwrap();

        client.ping(server_address);

        let client_thread = thread::spawn(move || loop {
            if let Some((message, from)) = client.tick() {
                assert_eq!(
                    message.message_type,
                    MessageType::Response(ResponseSpecific::PingResponse(PingResponseArguments {
                        responder_id: server_id
                    }))
                );
                assert_eq!(from.port(), server_address.port());
                return;
            }
        });

        client_thread.join().unwrap();
    }

    #[test]
    fn find_node_response() {
        let mut server = Rpc::new().unwrap();
        let server_address = server.local_addr();
        let server_id = server.id;

        let node = Node::random();
        server.routing_table.add(node.clone());

        thread::spawn(move || loop {
            server.tick();
        });

        let mut client = Rpc::new().unwrap();

        client.find_node(server_address, client.id);

        let client_thread = thread::spawn(move || loop {
            if let Some((message, from)) = client.tick() {
                assert_eq!(
                    message.message_type,
                    MessageType::Response(ResponseSpecific::FindNodeResponse(
                        FindNodeResponseArguments {
                            responder_id: server_id,
                            nodes: vec![node]
                        }
                    ))
                );
                assert_eq!(from.port(), server_address.port());
                return;
            }
        });

        client_thread.join().unwrap();
    }
}
