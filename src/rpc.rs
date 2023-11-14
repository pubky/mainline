use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::num::NonZeroUsize;
use std::thread;
use std::time::Duration;

use crate::common::{GetPeerResponse, Id, Node, ResponseFrom, ResponseItem, ResponseSender};
use crate::messages::{
    FindNodeRequestArguments, FindNodeResponseArguments, GetPeersRequestArguments,
    GetPeersResponseArguments, Message, MessageType, PingResponseArguments, RequestSpecific,
    ResponseSpecific,
};

use crate::peers::PeersStore;
use crate::query::Query;
use crate::routing_table::RoutingTable;
use crate::socket::KrpcSocket;
use crate::tokens::Tokens;
use crate::Result;

const TICK_INTERVAL: Duration = Duration::from_millis(15);
const QUERIES_CACHE_SIZE: usize = 1000;
const DEFAULT_BOOTSTRAP_NODES: [&str; 8] = [
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401", // @arvidn's
    "router.bittorrent.com:6881",
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
    queries: HashMap<Id, Query>,
    tokens: Tokens,
    peers: PeersStore,

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
            queries: HashMap::new(),
            tokens: Tokens::new(),
            peers: PeersStore::new(),
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

    pub fn tick(&mut self) {
        // === Bootstrapping ===
        self.populate();

        if let Some((message, from)) = self.socket.recv_from() {
            // Add a node to our routing table on any incoming request or response.
            self.add_node(&message, from);

            match &message.message_type {
                MessageType::Request(request_specific) => {
                    self.handle_request(from, message.transaction_id, request_specific);
                }
                MessageType::Response(_) => {
                    self.handle_response(from, &message);
                }
                MessageType::Error(_) => {
                    // TODO: Handle error messages!
                }
            }
        };

        // === Tick queries ===
        for (target, query) in self.queries.iter_mut() {
            query.tick(&mut self.socket);
        }

        // === Remove done queries ===
        self.queries.retain(|_, query| !query.is_done());

        thread::sleep(self.interval);
    }

    // Start or restart a get_peers query.
    pub fn get_peers(&mut self, info_hash: Id, sender: ResponseSender) {
        self.query(
            info_hash,
            RequestSpecific::GetPeers(GetPeersRequestArguments {
                requester_id: self.id,
                info_hash,
            }),
            Some(sender),
        )
    }

    // === Private Methods ===

    /// Send a message to closer and closer nodes until we can't find any more nodes.
    ///
    /// Queries take few seconds to traverse the network, once it is done, it will be removed from
    /// self.queries. But until then, calling `rpc.query()` multiple times, will just add the
    /// sender to the query, send all the responses seen so far, as well as subsequent responses.
    ///
    /// Effectively, we are caching responses and backing off the network for the duration it takes
    /// to traverse it.
    fn query(&mut self, target: Id, request: RequestSpecific, sender: Option<ResponseSender>) {
        // If query is still active, add the sender to it.
        if let Some(query) = self.queries.get_mut(&target) {
            query.add_sender(sender);
            return;
        }

        let mut query = Query::new(target, request);

        query.add_sender(sender);

        // Seed the query either with the closest nodes from the routing table, or the
        // bootstrapping nodes if the closest nodes are not enough.

        let closest = self.routing_table.closest(&target);

        // If we don't have enough or any closest nodes, call the bootstraping nodes.
        if closest.is_empty() || closest.len() < self.bootstrap.len() {
            for bootstrapping_node in self.bootstrap.clone() {
                if let Ok(addresses) = bootstrapping_node.to_socket_addrs() {
                    for address in addresses {
                        query.visit(&mut self.socket, address);
                    }
                }
            }
        } else {
            // Seed this query with the closest nodes we know about.
            for node in closest {
                query.add_node(node)
            }
        }

        // After adding the nodes, we need to start the query.
        query.start(&mut self.socket);

        self.queries.insert(target, query);
    }

    /// Ping bootstrap nodes, add them to the routing table with closest query.
    fn populate(&mut self) {
        if !self.routing_table.is_empty() {
            // No need for populating. Already called our bootstrap nodes?
            return;
        }

        // Start or restart the query.
        self.query(
            self.id,
            RequestSpecific::FindNode(FindNodeRequestArguments {
                target: self.id,
                requester_id: self.id,
            }),
            None,
        );
    }

    /// Return a boolean indicating whether the bootstrapping query is done.
    fn is_ready(&mut self) -> bool {
        return if let Some(query) = self.queries.get(&self.id) {
            query.is_done()
        } else {
            false
        };
    }

    fn handle_request(&mut self, from: SocketAddr, transaction_id: u16, request: &RequestSpecific) {
        match request {
            // TODO: Handle bad requests (send an error message).
            RequestSpecific::Ping(_) => {
                self.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::Ping(PingResponseArguments {
                        responder_id: self.id,
                    }),
                );
            }
            RequestSpecific::FindNode(FindNodeRequestArguments { target, .. }) => {
                self.socket.response(
                    from,
                    transaction_id,
                    ResponseSpecific::FindNode(FindNodeResponseArguments {
                        responder_id: self.id,
                        nodes: self.routing_table.closest(target),
                    }),
                );
            }
            RequestSpecific::GetPeers(GetPeersRequestArguments { info_hash, .. }) => {
                if let Some(values) = self.peers.get_random_peers(info_hash) {
                    self.socket.response(
                        from,
                        transaction_id,
                        ResponseSpecific::GetPeers(GetPeersResponseArguments {
                            responder_id: self.id,
                            token: self.tokens.generate_token(from).into(),
                            nodes: None,
                            values: Some(values),
                        }),
                    );
                } else {
                    self.socket.response(
                        from,
                        transaction_id,
                        ResponseSpecific::GetPeers(GetPeersResponseArguments {
                            responder_id: self.id,
                            token: self.tokens.generate_token(from).into(),
                            nodes: Some(self.routing_table.closest(info_hash)),
                            values: None,
                        }),
                    );
                }
            }
            _ => {
                // TODO: Handle queries (stuff with closer nodes in the response).
                // TODO: Send error message?
                // TODO: How to deal with unknown requests?
                // TODO: should we rsepond with FindNodeResponse anyways?
                todo!()
            }
        }
    }

    fn handle_response(&mut self, from: SocketAddr, message: &Message) {
        if message.read_only {
            return;
        }

        // Get corresponing query for message.transaction_id
        if let Some(query) = self.queries.iter_mut().find_map(|(_, query)| {
            if query.remove_inflight_request(message.transaction_id) {
                return Some(query);
            }
            None
        }) {
            if let Some(nodes) = message.get_closer_nodes() {
                for node in nodes {
                    query.add_node(node);
                }
            }

            match &message.message_type {
                MessageType::Response(ResponseSpecific::GetPeers(GetPeersResponseArguments {
                    responder_id,
                    token,
                    values: Some(peers),
                    ..
                })) => {
                    for peer in peers.clone() {
                        query.response(ResponseItem::Peer(GetPeerResponse {
                            from: ResponseFrom {
                                id: *responder_id,
                                address: from,
                                token: token.clone(),
                                version: message.version.clone(),
                            },
                            peer,
                        }));
                    }
                }
                // Ping response is already handled in add_node()
                // FindNode response is alreadh handled in query.add_candidates()
                _ => {}
            }
        }
    }

    fn add_node(&mut self, message: &Message, from: SocketAddr) {
        if message.read_only {
            return;
        }

        if let Some(id) = message.get_author_id() {
            self.routing_table.add(Node::new(id, from));
        }
    }
}
