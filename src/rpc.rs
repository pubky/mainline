use lru::LruCache;
use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::num::NonZeroUsize;
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
const QUERIES_CACHE_SIZE: usize = 1000;
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
    socket: KrpcSocket,
    routing_table: RoutingTable,
    bootstrapping: bool,
    queries: LruCache<Id, Query>,

    // Options
    id: Id,
    interval: Duration,
    bootstrap: Vec<String>,
}

impl Rpc {
    pub fn new() -> Result<Self> {
        // TODO: One day I might implement BEP42.
        let id = Id::random();

        let socket = KrpcSocket::new()?;

        Ok(Rpc {
            id,
            bootstrap: DEFAULT_BOOTSTRAP_NODES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            interval: TICK_INTERVAL,

            socket,
            routing_table: RoutingTable::new().with_id(id),
            bootstrapping: false,
            queries: LruCache::new(NonZeroUsize::new(QUERIES_CACHE_SIZE).unwrap()),
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

    pub fn with_bootstrap(mut self, bootstrap: Vec<String>) -> Self {
        self.bootstrap = bootstrap;
        self
    }

    pub fn with_interval(mut self, interval: u64) -> Self {
        self.interval = Duration::from_millis(interval);
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
        // === Bootstrapping ===
        self.populate();

        if let Some((message, from)) = self.socket.recv_from() {
            match &message.message_type {
                MessageType::Request(request_specific) => {
                    self.handle_request(from, message.transaction_id, request_specific);
                }
                MessageType::Response(response_specific) => {
                    self.handle_response(from, message.transaction_id, response_specific);
                }
                MessageType::Error(_) => {
                    // TODO: Handle error messages!
                }
            }
        };

        // === Advance queries ===
        // for query in self.queries.values() {
        //     for node in query.closest.iter() {
        //         self.socket.request(node.address, &query.request);
        //     }
        // }

        thread::sleep(self.interval);
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
        // If query exists and it's set to done, restart it.
        if let Some(query) = self.queries.get_mut(&target) {
            query.restart();
            return;
        }

        let query = Query::new(target, request);
        self.queries.put(target, query);
    }

    /// Ping bootstrap nodes, add them to the routing table with closest query.
    pub fn populate(&mut self) {
        if !self.routing_table.is_empty() || self.bootstrapping {
            return;
        }

        self.bootstrapping = true;

        for bootstrapping_node in self.bootstrap.clone() {
            if let Ok(addresses) = bootstrapping_node.to_socket_addrs() {
                for address in addresses {
                    if address.is_ipv6() {
                        // TODO: Add support for IPV6.
                        continue;
                    }
                    self.find_node(address, self.id);
                }
            }
        }
    }

    // === Private Methods ===

    fn handle_request(&mut self, from: SocketAddr, transaction_id: u16, request: &RequestSpecific) {
        match request {
            // TODO: Handle bad requests (send an error message).
            RequestSpecific::PingRequest(PingRequestArguments { requester_id }) => {
                self.update_routing_table(*requester_id, from, None);

                self.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::PingResponse(PingResponseArguments {
                        responder_id: self.id,
                    }),
                );
            }
            RequestSpecific::FindNodeRequest(FindNodeRequestArguments {
                target,
                requester_id,
            }) => {
                self.update_routing_table(*requester_id, from, None);

                self.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::FindNodeResponse(FindNodeResponseArguments {
                        responder_id: self.id,
                        nodes: self.routing_table.closest(target),
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
    }

    fn handle_response(
        &mut self,
        from: SocketAddr,
        transaction_id: u16,
        response: &ResponseSpecific,
    ) {
        // println!("Got response from {:?}:\n === {:?}\n", &from, &message);

        match response {
            ResponseSpecific::PingResponse(PingResponseArguments { responder_id }) => {
                self.update_routing_table(*responder_id, from, None);
            }
            //  === Responses to queries with closer nodes. ===
            ResponseSpecific::FindNodeResponse(FindNodeResponseArguments {
                responder_id,
                nodes,
            }) => {
                self.update_routing_table(*responder_id, from, Some(nodes));
                // TODO: check a corresponding query
            }
            _ => {}
        }
    }

    fn update_routing_table(&mut self, id: Id, from: SocketAddr, nodes: Option<&Vec<Node>>) {
        self.routing_table.add(Node::new(id, from));

        if let Some(nodes) = nodes {
            for node in nodes {
                if !self.routing_table.contains(&node.id) {
                    self.ping(node.address)
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bootstrap() {
        let mut bootstrap: Vec<String> = Vec::with_capacity(1);

        for i in 0..10 {
            let mut rpc = Rpc::new().unwrap();

            if i > 0 {
                rpc = rpc.with_bootstrap(bootstrap.clone()).with_interval(10);
            } else {
                rpc = rpc.with_bootstrap(vec![]).with_interval(10);
                &bootstrap.push(format!("0.0.0.0:{}", rpc.local_addr().port()));
            }

            let handler = thread::spawn(move || loop {
                rpc.tick();
            });
        }

        let mut client = Rpc::new()
            .unwrap()
            .with_bootstrap(bootstrap)
            .with_interval(10);

        let client_thread = thread::spawn(move || {
            let started = Instant::now();
            loop {
                client.tick();
                if client.routing_table.to_vec().len() == 10 {
                    return;
                }
                if started.elapsed().as_millis() > 300 {
                    assert!(false, "Should have added all nodes to the routing table");
                }
            }
        });

        client_thread.join().unwrap();
    }
}
