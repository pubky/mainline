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
use flume::Sender;
use lru::LruCache;
use tracing::{debug, error};

use crate::common::{validate_immutable, Id, MutableItem, Node, RoutingTable};
use crate::messages::{
    FindNodeRequestArguments, GetImmutableResponseArguments, GetMutableResponseArguments,
    GetPeersResponseArguments, GetValueRequestArguments, Message, MessageType,
    PingResponseArguments, PutRequestSpecific, RequestSpecific, RequestTypeSpecific,
    ResponseSpecific,
};

pub use response::{
    GetImmutableResponse, GetMutableResponse, GetPeerResponse, Response, ResponseDone,
    ResponseMessage, ResponseSender, ResponseValue, StoreQueryMetdata,
};

use crate::Result;
use query::{Query, StoreQuery};
use server::{handle_request, PeersStore, Tokens};
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
const MAX_CACHED_BUCKETS: usize = 1000;

#[derive(Debug)]
pub struct Rpc {
    // Options
    id: Id,
    bootstrap: Vec<String>,

    socket: KrpcSocket,

    // Routing
    /// Closest nodes to this node
    routing_table: RoutingTable,
    /// Last time we refreshed the routing table with a find_node query.
    last_table_refresh: Instant,
    /// Last time we pinged nodes in the routing table.
    last_table_ping: Instant,
    /// Closest nodes to specific target
    closest_nodes: LruCache<Id, Vec<Node>>,

    // Active Queries
    queries: HashMap<Id, Query>,
    store_queries: HashMap<Id, StoreQuery>,

    tokens: Tokens,

    // server storage
    peers: PeersStore,
    immutable_values: LruCache<Id, Bytes>,
    mutable_values: LruCache<Id, MutableItem>,
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
            closest_nodes: LruCache::new(NonZeroUsize::new(MAX_CACHED_BUCKETS).unwrap()),

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

    pub fn id(&self) -> Id {
        self.id
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr()
    }

    /// Returns a clone of the routing_table.
    pub fn routing_table(&self) -> RoutingTable {
        self.routing_table.clone()
    }

    /// Returns a clone of the routing_table size.
    pub fn routing_table_size(&self) -> usize {
        self.routing_table.size()
    }

    // === Public Methods ===

    pub fn tick(&mut self) {
        // === Tokens ===
        if self.tokens.should_update() {
            self.tokens.rotate()
        }

        // === Tick Queries ===
        // Advance queries one step at a time.
        for (id, query) in self.store_queries.iter_mut() {
            if !query.alredy_started() {
                // Closest nodes if they exist and still have valid tokens
                if let Some(closest_nodes) = self
                    .closest_nodes
                    .get(id)
                    .filter(|nodes| !nodes.is_empty() && nodes.iter().any(|n| n.valid_token()))
                {
                    query.start(&mut self.socket, closest_nodes.to_vec())
                };
            };

            query.tick(&mut self.socket);
        }
        for (_, query) in self.queries.iter_mut() {
            query.tick(&mut self.socket);
        }

        // === Remove done queries ===
        // Has to happen _after_ ticking queries otherwise we might
        // disconnect response receivers too soon.
        //
        // Has to happen _before_ `self.socket.recv_from()`.
        self.maintain_queries();
        // Refresh the routing table, ping stale nodes, and remove unresponsive ones.
        self.maintain_routing_table();

        if let Some((message, from)) = self.socket.recv_from() {
            // Add a node to our routing table on any incoming request or response.
            self.add_node(&message, from);

            match &message.message_type {
                MessageType::Request(request_specific) => {
                    handle_request(self, from, message.transaction_id, request_specific)
                }
                MessageType::Response(_) => {
                    self.handle_response(from, &message);
                }
                MessageType::Error(error) => {
                    debug!(?error, "RPC Error response");
                }
            }
        };
    }

    /// Store a value in the closest nodes, optionally trigger a lookup query if
    /// the cached closest_nodes aren't fresh enough.
    ///
    /// `salt` is only relevant for mutable values.
    pub fn put(
        &mut self,
        target: Id,
        request: PutRequestSpecific,
        sender: Option<Sender<StoreQueryMetdata>>,
    ) {
        let mut query = StoreQuery::new(target, request.clone(), sender);

        // TODO: repeated code in the tick()
        if let Some(closest_nodes) = self
            .closest_nodes
            .get(&target)
            .filter(|nodes| !nodes.is_empty() && nodes.iter().any(|n| n.valid_token()))
        {
            query.start(&mut self.socket, closest_nodes.to_vec())
        } else {
            let salt = match request {
                PutRequestSpecific::PutMutable(args) => args.salt,
                _ => None,
            };

            self.get(
                target,
                RequestTypeSpecific::GetValue(GetValueRequestArguments {
                    target,
                    seq: None,
                    salt: salt.map(|s| s.into()),
                }),
                None,
            );
        };

        self.store_queries.insert(target, query);
    }

    /// Send a message to closer and closer nodes until we can't find any more nodes.
    ///
    /// Queries take few seconds to fully traverse the network, once it is done, it will be removed from
    /// self.queries. But until then, calling `rpc.query()` multiple times, will just add the
    /// sender to the query, send all the responses seen so far, as well as subsequent responses.
    ///
    /// Effectively, we are caching responses and backing off the network for the duration it takes
    /// to traverse it.
    pub fn get(
        &mut self,
        target: Id,
        request: RequestTypeSpecific,
        sender: Option<ResponseSender>,
    ) {
        // If query is still active, add the sender to it.
        if let Some(query) = self.queries.get_mut(&target) {
            query.add_sender(sender);
            return;
        }

        let mut query = Query::new(
            target,
            RequestSpecific {
                requester_id: self.id,
                request_type: request,
            },
        );

        query.add_sender(sender);

        // Seed the query either with the closest nodes from the routing table, or the
        // bootstrapping nodes if the closest nodes are not enough.

        let routing_table_closest = self.routing_table.closest(&target);

        // If we don't have enough or any closest nodes, call the bootstraping nodes.
        if routing_table_closest.is_empty() || routing_table_closest.len() < self.bootstrap.len() {
            for bootstrapping_node in self.bootstrap.clone() {
                if let Ok(addresses) = bootstrapping_node.to_socket_addrs() {
                    for address in addresses {
                        query.visit(&mut self.socket, address);
                    }
                }
            }
        }

        // Seed this query with the closest nodes we know about.
        for node in routing_table_closest {
            query.add_candidate(node)
        }

        if let Some(cached_closest) = self.closest_nodes.get(&target) {
            for node in cached_closest {
                query.add_candidate(node.clone())
            }
        }

        // After adding the nodes, we need to start the query.
        query.start(&mut self.socket);

        self.queries.insert(target, query);
    }

    // === Private Methods ===

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
                    let salt = match query.request().request_type.clone() {
                        RequestTypeSpecific::GetValue(args) => args.salt,
                        _ => None,
                    };
                    let target = query.target();

                    if let Ok(item) = MutableItem::from_dht_message(
                        query.target(),
                        k,
                        v.to_owned().into(),
                        seq,
                        sig,
                        salt.to_owned(),
                        &None,
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

    fn maintain_queries(&mut self) {
        let self_id = self.id;
        let table_size = self.routing_table.size();

        let mut closest_nodes = Vec::with_capacity(self.queries.len());
        self.queries.retain(|id, query| {
            let done = query.is_done();

            if done {
                closest_nodes.push((*id, query.closest()));

                if id == &self_id {
                    if table_size == 0 {
                        error!("Could not bootstrap the routing table");
                    } else {
                        debug!(table_size, "Populated the routing table");
                    }
                }
            }

            !done
        });
        for (id, nodes) in closest_nodes {
            self.closest_nodes.put(id, nodes);
        }
        self.store_queries.retain(|_, query| !query.is_done());
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
        self.get(
            self.id,
            RequestTypeSpecific::FindNode(FindNodeRequestArguments { target: self.id }),
            None,
        );
    }

    fn ping(&mut self, address: SocketAddr) {
        self.socket.request(
            address,
            RequestSpecific {
                requester_id: self.id,
                request_type: RequestTypeSpecific::Ping,
            },
        );
    }
}
