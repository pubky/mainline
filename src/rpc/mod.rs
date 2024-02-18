//! K-RPC implementation

mod query;
pub mod response;
mod server;
mod socket;

use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use bytes::Bytes;
use lru::LruCache;
use tracing::{debug, error};

use crate::common::{validate_immutable, Id, MutableItem, Node, RoutingTable};
use crate::messages::{
    AnnouncePeerRequestArguments, ErrorSpecific, FindNodeRequestArguments,
    FindNodeResponseArguments, GetImmutableRequestArguments, GetImmutableResponseArguments,
    GetMutableRequestArguments, GetMutableResponseArguments, GetPeersRequestArguments,
    GetPeersResponseArguments, Message, MessageType, NoValuesResponseArguments,
    PingRequestArguments, PingResponseArguments, PutImmutableRequestArguments,
    PutMutableRequestArguments, RequestSpecific, ResponseSpecific,
};

pub use response::{
    GetImmutableResponse, GetMutableResponse, GetPeerResponse, Response, ResponseDone,
    ResponseMessage, ResponseSender, ResponseValue, StoreQueryMetdata,
};

use crate::Result;
use query::{Query, StoreQuery};
use server::{PeersStore, Tokens};
use socket::KrpcSocket;

const DEFAULT_BOOTSTRAP_NODES: [&str; 4] = [
    "router.bittorrent.com:6881",
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401",
    "dht.anacrolix.link:42069",
];

const REFRESH_TABLE_INTERVAL: Duration = Duration::from_secs(15 * 60);
const PING_TABLE_INTERVAL: Duration = Duration::from_secs(5 * 60);

// Stored data in server mode.
const MAX_INFO_HASHES: usize = 2000;
const MAX_PEERS: usize = 500;
const MAX_VALUES: usize = 1000;

// TODO: RpcBuilder with settings here and the DhtBuilder.
// TODO: store and read to local stores, at least for immutable and mutable values.

#[derive(Debug)]
pub struct Rpc {
    socket: KrpcSocket,
    routing_table: RoutingTable,
    queries: HashMap<Id, Query>,
    store_queries: HashMap<Id, StoreQuery>,
    tokens: Tokens,

    peers: PeersStore,
    immutable_values: LruCache<Id, Bytes>,
    mutable_values: LruCache<Id, MutableItem>,

    /// Last time we refreshed the routing table with a find_node query.
    last_table_refresh: Instant,
    /// Last time we pinged nodes in the routing table.
    last_table_ping: Instant,

    // Options
    id: Id,
    bootstrap: Vec<String>,
}

impl Rpc {
    pub fn new() -> Result<Self> {
        // TODO: One day I might implement BEP42 on Routing nodes.
        let id = Id::random();

        let socket = KrpcSocket::new()?;

        Ok(Rpc {
            id,
            bootstrap: DEFAULT_BOOTSTRAP_NODES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            socket,
            routing_table: RoutingTable::new().with_id(id),
            queries: HashMap::new(),
            store_queries: HashMap::new(),
            tokens: Tokens::new(),

            peers: PeersStore::new(
                NonZeroUsize::new(MAX_INFO_HASHES).unwrap(),
                NonZeroUsize::new(MAX_PEERS).unwrap(),
            ),
            immutable_values: LruCache::new(NonZeroUsize::new(MAX_VALUES).unwrap()),
            mutable_values: LruCache::new(NonZeroUsize::new(MAX_VALUES).unwrap()),

            last_table_refresh: Instant::now() - REFRESH_TABLE_INTERVAL,
            last_table_ping: Instant::now(),
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
        // === Tick Queries ===
        for (_, query) in self.queries.iter_mut() {
            query.tick(&mut self.socket);
        }
        for (_, query) in self.store_queries.iter_mut() {
            query.tick(&mut self.socket);
        }

        // === Remove done queries ===
        // Has to happen _after_ ticking queries otherwise we might
        // disconnect response receivers too soon.
        //
        // Has to happen _before_ await to recv_from the socket.
        let self_id = self.id;
        let table_size = self.routing_table.size();

        self.queries.retain(|id, query| {
            let done = query.is_done();

            if done && id == &self_id {
                if table_size == 0 {
                    error!("Could not bootstrap the routing table");
                } else {
                    debug!(table_size, "Populated the routing table");
                }
            }

            !done
        });
        self.store_queries.retain(|_, query| !query.is_done());

        self.maintain_routing_table();

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
                MessageType::Error(error) => {
                    dbg!(&error);
                    debug!(?message, "RPC Error response");
                }
            }
        };
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

        let mut query = StoreQuery::new(info_hash, sender);

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
            RequestSpecific::GetImmutable(GetImmutableRequestArguments {
                requester_id: self.id,
                target,
            }),
            Some(sender),
        )
    }

    pub fn put_immutable(
        &mut self,
        target: Id,
        value: Bytes,
        nodes: Vec<Node>,
        sender: ResponseSender,
    ) {
        let mut query = StoreQuery::new(target, sender);

        for node in nodes {
            if let Some(token) = node.token.clone() {
                query.request(
                    node,
                    RequestSpecific::PutImmutable(PutImmutableRequestArguments {
                        requester_id: self.id,
                        target,
                        token,
                        v: value.clone().into(),
                    }),
                    &mut self.socket,
                );
            }
        }

        self.store_queries.insert(target, query);
    }

    pub fn get_mutable(&mut self, target: Id, salt: Option<Bytes>, sender: ResponseSender) {
        self.query(
            target,
            RequestSpecific::GetMutable(GetMutableRequestArguments {
                requester_id: self.id,
                target,
                salt,
            }),
            Some(sender),
        )
    }

    pub fn put_mutable(&mut self, item: MutableItem, nodes: Vec<Node>, sender: ResponseSender) {
        let mut query = StoreQuery::new(*item.target(), sender);

        for node in nodes {
            if let Some(token) = node.token.clone() {
                query.request(
                    node,
                    RequestSpecific::PutMutable(PutMutableRequestArguments {
                        requester_id: self.id,
                        target: *item.target(),
                        token,
                        v: item.value().clone().into(),
                        k: item.key().to_vec(),
                        seq: *item.seq(),
                        sig: item.signature().to_vec(),
                        salt: item.salt().clone().map(|s| s.to_vec()),
                    }),
                    &mut self.socket,
                );
            }
        }

        self.store_queries.insert(*item.target(), query);
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

    fn handle_request(&mut self, from: SocketAddr, transaction_id: u16, request: &RequestSpecific) {
        match request {
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
                requester_id,
                ..
            }) => {
                if self.tokens.validate(from, token) {
                    let peer = match implied_port {
                        Some(true) => from,
                        _ => SocketAddr::new(from.ip(), *port),
                    };

                    self.peers.add_peer(*info_hash, (requester_id, peer));

                    self.socket.response(
                        from,
                        transaction_id,
                        ResponseSpecific::Ping(PingResponseArguments {
                            responder_id: self.id,
                        }),
                    );
                } else {
                    self.socket.error(
                        from,
                        transaction_id,
                        ErrorSpecific {
                            code: 203,
                            description: "Bad token".to_string(),
                        },
                    );
                    debug!(?from, ?token, "Invalid token");
                }
            }
            RequestSpecific::PutImmutable(PutImmutableRequestArguments { v, target, .. }) => {
                if v.len() > 1000 {
                    self.socket.error(
                        from,
                        transaction_id,
                        ErrorSpecific {
                            code: 205,
                            description: "Message (v field) too big.".to_string(),
                        },
                    );
                    return;
                }
                if !validate_immutable(v, target) {
                    self.socket.error(
                        from,
                        transaction_id,
                        ErrorSpecific {
                            code: 203,
                            description: "Target doesn't match the sha1 hash of v field"
                                .to_string(),
                        },
                    );
                    return;
                }

                self.immutable_values.put(*target, v.to_owned().into());
            }
            RequestSpecific::GetImmutable(GetImmutableRequestArguments {
                requester_id,
                target,
            }) => self.socket.response(
                from,
                transaction_id,
                match self.immutable_values.get(target) {
                    Some(v) => ResponseSpecific::GetImmutable(GetImmutableResponseArguments {
                        responder_id: self.id,
                        token: self.tokens.generate_token(from).into(),
                        nodes: Some(self.routing_table.closest(target)),
                        v: v.to_vec(),
                    }),
                    None => ResponseSpecific::NoValues(NoValuesResponseArguments {
                        responder_id: self.id,
                        token: self.tokens.generate_token(from).into(),
                        nodes: Some(self.routing_table.closest(target)),
                    }),
                },
            ),
            RequestSpecific::PutMutable(PutMutableRequestArguments {
                requester_id,
                target,
                token,
                v,
                k,
                seq,
                sig,
                salt,
            }) => {
                if v.len() > 1000 {
                    self.socket.error(
                        from,
                        transaction_id,
                        ErrorSpecific {
                            code: 205,
                            description: "Message (v field) too big.".to_string(),
                        },
                    );
                    // return;
                }
            }
            _ => {
                self.socket.error(
                    from,
                    transaction_id,
                    ErrorSpecific {
                        code: 204,
                        description: "Method Unknown".to_string(),
                    },
                );
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
                // Mark storage at that node as a success.
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
                        query.response(ResponseValue::Peer(GetPeerResponse {
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
                        let target = query.target();
                        debug!(?v, ?target, "Invalid immutable value");
                        return;
                    }

                    query.response(ResponseValue::Immutable(GetImmutableResponse {
                        from: Node::new(*responder_id, from),
                        value: v.to_owned().into(),
                    }));
                }
                MessageType::Response(ResponseSpecific::GetMutable(
                    GetMutableResponseArguments {
                        responder_id,
                        v,
                        seq,
                        sig,
                        k,
                        ..
                    },
                )) => {
                    let salt = match query.request() {
                        RequestSpecific::GetMutable(GetMutableRequestArguments {
                            salt, ..
                        }) => salt,
                        _ => &None,
                    };
                    let target = query.target();

                    if let Ok(item) = MutableItem::from_dht_message(
                        query.target(),
                        k,
                        v.to_owned().into(),
                        seq,
                        sig,
                        salt,
                    ) {
                        query.response(ResponseValue::Mutable(GetMutableResponse {
                            from: Node::new(*responder_id, from),
                            item,
                        }));
                    } else {
                        debug!(?v, ?seq, ?sig, ?salt, ?target, "Invalid mutable record");
                    }
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

    fn maintain_routing_table(&mut self) {
        if self.routing_table.is_empty()
            && self.last_table_refresh.elapsed() > REFRESH_TABLE_INTERVAL
        {
            self.last_table_refresh = Instant::now();
            self.populate();
        }

        if self.last_table_ping.elapsed() > PING_TABLE_INTERVAL {
            self.last_table_ping = Instant::now();

            for node in self.routing_table.to_vec() {
                if node.is_stale() {
                    self.routing_table.remove(&node.id);
                } else if node.should_ping() {
                    self.ping(node.address);
                }
            }
        }
    }

    /// Ping bootstrap nodes, add them to the routing table with closest query.
    fn populate(&mut self) {
        self.query(
            self.id,
            RequestSpecific::FindNode(FindNodeRequestArguments {
                target: self.id,
                requester_id: self.id,
            }),
            None,
        );
    }

    fn ping(&mut self, address: SocketAddr) {
        self.socket.request(
            address,
            RequestSpecific::Ping(PingRequestArguments {
                requester_id: self.id,
            }),
        );
    }
}
