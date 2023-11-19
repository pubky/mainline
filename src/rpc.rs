use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::thread;
use std::time::Duration;

use sha1_smol::Sha1;

use crate::common::{
    GetImmutableResponse, GetPeerResponse, Id, Node, ResponseSender, ResponseValue,
};
use crate::messages::{
    AnnouncePeerRequestArguments, FindNodeRequestArguments, FindNodeResponseArguments,
    GetImmutableResponseArguments, GetPeersRequestArguments, GetPeersResponseArguments,
    GetValueRequestArguments, Message, MessageType, NoValuesResponseArguments,
    PingResponseArguments, RequestSpecific, ResponseSpecific,
};

use crate::peers::PeersStore;
use crate::query::{Query, StoreQuery};
use crate::routing_table::RoutingTable;
use crate::socket::KrpcSocket;
use crate::tokens::Tokens;
use crate::Result;

const TICK_INTERVAL: Duration = Duration::from_millis(1);
const DEFAULT_BOOTSTRAP_NODES: [&str; 4] = [
    "router.bittorrent.com:6881",
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401",
    "dht.anacrolix.link:42069",
];

#[derive(Debug)]
pub struct Rpc {
    socket: KrpcSocket,
    routing_table: RoutingTable,
    queries: HashMap<Id, Query>,
    store_queries: HashMap<Id, StoreQuery>,
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
            store_queries: HashMap::new(),
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

    pub fn with_port(mut self, port: u16) -> Result<Self> {
        self.socket = KrpcSocket::bind(port)?;
        Ok(self)
    }

    /// Sets requests timeout in milliseconds
    pub fn with_request_timeout(mut self, timeout: u64) -> Self {
        self.socket.request_timeout = Duration::from_millis(timeout);
        self
    }

    // === Getters ===

    /// Returns the address the server is listening to.
    #[inline]
    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr()
    }

    /// Returns a clone of the routing_table.
    pub fn routing_table(&self) -> RoutingTable {
        self.routing_table.clone()
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
                MessageType::Error(_err) => {
                    // TODO: Handle error messages!
                }
            }
        };

        // === Tick queries ===
        for (_, query) in self.queries.iter_mut() {
            query.tick(&mut self.socket);
        }
        for (_, query) in self.store_queries.iter_mut() {
            query.tick(&mut self.socket);
        }

        // === Remove done queries ===
        self.queries.retain(|_, query| !query.is_done());
        self.store_queries.retain(|_, query| !query.is_done());

        thread::sleep(self.interval);
    }

    /// Start or restart a get_peers query.
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

    /// Send an announce_peer request to a list of nodes.
    pub fn announce_peer(
        &mut self,
        info_hash: Id,
        nodes: Vec<Node>,
        port: Option<u16>,
        sender: ResponseSender,
    ) {
        let (port, implied_port) = match port {
            Some(port) => (port, None),
            None => (0, Some(true)),
        };

        let mut query = StoreQuery::new(sender);

        for node in nodes {
            if let Some(token) = node.token.clone() {
                query.request(
                    node,
                    RequestSpecific::AnnouncePeer(AnnouncePeerRequestArguments {
                        requester_id: self.id,
                        info_hash,
                        port,
                        implied_port,
                        token,
                    }),
                    &mut self.socket,
                );
            }
        }

        self.store_queries.insert(info_hash, query);
    }

    pub fn get_immutable(&mut self, target: Id, sender: ResponseSender) {
        self.query(
            target,
            RequestSpecific::GetValue(GetValueRequestArguments {
                requester_id: self.id,
                target,
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
                query.add_candidate(node)
            }

            // After adding the nodes, we need to start the query.
            query.start(&mut self.socket);
        }

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
                self.socket.response(
                    from,
                    transaction_id,
                    match self.peers.get_random_peers(info_hash) {
                        Some(peers) => ResponseSpecific::GetPeers(GetPeersResponseArguments {
                            responder_id: self.id,
                            token: self.tokens.generate_token(from).into(),
                            nodes: Some(self.routing_table.closest(info_hash)),
                            values: peers,
                        }),
                        None => ResponseSpecific::NoValues(NoValuesResponseArguments {
                            responder_id: self.id,
                            token: self.tokens.generate_token(from).into(),
                            nodes: Some(self.routing_table.closest(info_hash)),
                        }),
                    },
                );
            }
            RequestSpecific::AnnouncePeer(AnnouncePeerRequestArguments {
                info_hash,
                port,
                implied_port,
                token,
                ..
            }) => {
                if self.tokens.validate(from, token) {
                    let peer = match implied_port {
                        Some(true) => from,
                        _ => SocketAddr::new(from.ip(), *port),
                    };

                    self.peers.add_peer(*info_hash, peer);

                    self.socket.response(
                        from,
                        transaction_id,
                        ResponseSpecific::Ping(PingResponseArguments {
                            responder_id: self.id,
                        }),
                    );
                } else {
                    // TODO: Send an error message.
                }
            }
            _ => {
                // TODO: How to deal with unknown requests?
                // Maybe just return CloserNodesAndToken to the sender?
            }
        }
    }

    fn handle_response(&mut self, from: SocketAddr, message: &Message) {
        if message.read_only {
            return;
        }

        // If the response looks like a Ping response, check StoreQueries for the transaction_id.
        if let Some(query) = self.store_queries.iter_mut().find_map(|(_, query)| {
            if query.remove_inflight_request(message.transaction_id) {
                return Some(query);
            }
            None
        }) {
            if let MessageType::Response(ResponseSpecific::Ping(PingResponseArguments {
                responder_id,
            })) = message.message_type
            {
                query.success(responder_id);
            }

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
                    query.add_candidate(node);
                }
            }

            if let Some((responder_id, token)) = message.get_token() {
                query.add_responding_node(Node::new(responder_id, from).with_token(token.clone()));
            }

            match &message.message_type {
                MessageType::Response(ResponseSpecific::GetPeers(GetPeersResponseArguments {
                    responder_id,
                    values,
                    ..
                })) => {
                    for peer in values.clone() {
                        query.response(ResponseValue::GetPeer(GetPeerResponse {
                            from: Node::new(*responder_id, from),
                            peer,
                        }));
                    }
                }
                MessageType::Response(ResponseSpecific::GetImmutable(
                    GetImmutableResponseArguments {
                        responder_id, v, ..
                    },
                )) => {
                    if !validate_immutable(v, query.target()) {
                        // TODO: log error
                        return;
                    }

                    query.response(ResponseValue::GetImmutable(GetImmutableResponse {
                        from: Node::new(*responder_id, from),
                        value: v.clone(),
                    }));
                }
                // Ping response is already handled in add_node()
                // FindNode response is already handled in query.add_candidate()
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

fn validate_immutable(v: &[u8], target: Id) -> bool {
    let mut encoded = Vec::with_capacity(v.len() + 3);
    encoded.extend_from_slice(b"20:");
    encoded.extend_from_slice(v);

    let mut hasher = Sha1::new();
    hasher.update(&encoded);
    let hash = hasher.digest().bytes();

    hash == target.bytes
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_validate_immutable() {
        let v = vec![
            171, 118, 111, 111, 174, 109, 195, 32, 138, 140, 113, 176, 76, 135, 116, 132, 156, 126,
            75, 173,
        ];

        let target = Id::from_bytes(&[
            2, 23, 113, 43, 67, 11, 185, 26, 26, 30, 204, 238, 204, 1, 13, 84, 52, 40, 86, 231,
        ])
        .unwrap();

        assert!(validate_immutable(&v, target));
        assert!(!validate_immutable(&v[1..], target));
    }
}
