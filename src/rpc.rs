//! K-RPC implementatioStoreQueryMetdatan

mod closest_nodes;
mod query;
mod socket;

use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs};
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use bytes::Bytes;
use flume::Sender;
use lru::LruCache;
use tracing::{debug, error, info};

use crate::common::{
    validate_immutable, ErrorSpecific, FindNodeRequestArguments, GetImmutableResponseArguments,
    GetMutableResponseArguments, GetPeersResponseArguments, GetValueRequestArguments, Id, Message,
    MessageType, MutableItem, NoMoreRecentValueResponseArguments, NoValuesResponseArguments, Node,
    PutRequestSpecific, RequestSpecific, RequestTypeSpecific, ResponseSpecific, RoutingTable,
};

use crate::dht::Settings;
use query::{PutQuery, Query};
use socket::KrpcSocket;

pub use crate::common::messages;
pub use closest_nodes::ClosestNodes;
pub use query::PutError;
pub use socket::DEFAULT_PORT;
pub use socket::DEFAULT_REQUEST_TIMEOUT;

pub const DEFAULT_BOOTSTRAP_NODES: [&str; 4] = [
    "router.bittorrent.com:6881",
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401",
    "dht.anacrolix.link:42069",
];

const REFRESH_TABLE_INTERVAL: Duration = Duration::from_secs(15 * 60);
const PING_TABLE_INTERVAL: Duration = Duration::from_secs(5 * 60);

const MAX_CACHED_BUCKETS: usize = 1000;

// If you are making a FIND_NODE requests subsequentially,
// you can expect the oldest sample to be an hour ago.
const DHT_SIZE_ESTIMATE_WINDOW: i32 = 1024;

#[derive(Debug)]
/// Internal Rpc called in the Dht thread loop, useful to create your own actor setup.
pub struct Rpc {
    // Options
    id: Id,
    bootstrap: Vec<SocketAddr>,

    socket: KrpcSocket,

    // Routing
    /// Closest nodes to this node
    routing_table: RoutingTable,
    /// Last time we refreshed the routing table with a find_node query.
    last_table_refresh: Instant,
    /// Last time we pinged nodes in the routing table.
    last_table_ping: Instant,
    /// Closest nodes to specific target
    closest_nodes: LruCache<Id, ClosestNodes>,

    // Active Queries
    queries: HashMap<Id, Query>,
    /// Put queries are special, since they have to wait for a corresponing
    /// get query to finish, update the closest_nodes, then `query_all` these.
    put_queries: HashMap<Id, PutQuery>,

    /// Moving average of the estimated dht size from the lookups within a [DHT_SIZE_ESTIMATE_WINDOW]
    dht_size_estimate: i32,
    dht_size_estimate_samples: i32,
}

impl Rpc {
    /// Create a new Rpc
    pub fn new(settings: &Settings) -> Result<Self, std::io::Error> {
        // TODO: One day I might implement BEP42 on Routing nodes.
        let id = Id::random();

        let socket = KrpcSocket::new(settings)?;

        info!(?settings, "Sarting Mainline Rpc");

        Ok(Rpc {
            id,
            bootstrap: settings
                .bootstrap
                .to_owned()
                .unwrap_or(
                    DEFAULT_BOOTSTRAP_NODES
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                )
                .iter()
                .flat_map(|s| s.to_socket_addrs().map(|addrs| addrs.collect::<Vec<_>>()))
                .flatten()
                .collect::<Vec<_>>(),
            socket,
            routing_table: RoutingTable::new().with_id(id),
            queries: HashMap::new(),
            put_queries: HashMap::new(),
            closest_nodes: LruCache::new(
                NonZeroUsize::new(MAX_CACHED_BUCKETS).expect("MAX_CACHED_BUCKETS is NonZeroUsize"),
            ),

            last_table_refresh: Instant::now()
                .checked_sub(REFRESH_TABLE_INTERVAL)
                .unwrap_or_else(Instant::now),
            last_table_ping: Instant::now(),

            dht_size_estimate: 0,
            dht_size_estimate_samples: 0,
        })
    }

    // === Options ===

    /// Override the Rpc's Id which is set randomly be default.
    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    // === Getters ===

    /// Returns the node's Id
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.socket.local_addr()
    }

    pub fn routing_table(&self) -> &RoutingTable {
        &self.routing_table
    }

    pub fn dht_size_estimate(&self) -> usize {
        self.dht_size_estimate as usize
    }

    // === Public Methods ===

    /// Advance the inflight queries, receive incoming requests,
    /// maintain the routing table, and everything else that needs
    /// to happen at every tick.
    pub fn tick(&mut self) -> RpcTickReport {
        // === Tick Queries ===

        let mut done_get_queries = Vec::with_capacity(self.queries.len());
        let mut done_put_queries = Vec::with_capacity(self.put_queries.len());

        for (id, query) in self.put_queries.iter_mut() {
            let done = query.tick(&mut self.socket);

            if done {
                done_put_queries.push(*id);
            }
        }

        let self_id = self.id;
        let table_size = self.routing_table.size();

        for (id, query) in self.queries.iter_mut() {
            let is_done = query.tick(&mut self.socket);

            if is_done {
                let closest_nodes = query.closest_nodes();

                // Calculate moving average
                let estimate = closest_nodes.dht_size_estimate() as i32;
                self.dht_size_estimate_samples =
                    (self.dht_size_estimate_samples + 1).min(DHT_SIZE_ESTIMATE_WINDOW);

                // TODO: warn if a Horizontal Sybil attack is deteceted.

                self.dht_size_estimate = self.dht_size_estimate
                    + (estimate - self.dht_size_estimate) / self.dht_size_estimate_samples;

                if let Some(put_query) = self.put_queries.get_mut(id) {
                    put_query.start(&mut self.socket, closest_nodes.nodes())
                }

                self.closest_nodes.put(*id, closest_nodes.clone());
                done_get_queries.push(*id);

                if id == &self_id && table_size == 0 {
                    error!("Could not bootstrap the routing table");
                } else {
                    debug!(table_size, "Populated the routing table");
                };
            };
        }

        // === Remove done queries ===
        // Has to happen _after_ ticking queries otherwise we might
        // disconnect response receivers too soon.
        //
        // Has to happen _before_ `self.socket.recv_from()`.
        for id in &done_get_queries {
            self.queries.remove(id);
        }
        for id in &done_put_queries {
            self.put_queries.remove(id);
        }

        // Refresh the routing table, ping stale nodes, and remove unresponsive ones.
        self.maintain_routing_table();

        let received_from = self.socket.recv_from().and_then(|(message, from)| {
            // Add a node to our routing table on any incoming request or response.
            self.add_node(&message, from);

            match &message.message_type {
                MessageType::Request(request_specific) => Some(ReceivedFrom {
                    from,
                    message: ReceivedMessage::Request((
                        message.transaction_id,
                        request_specific.clone(),
                    )),
                }),
                _ => self
                    .handle_response(from, &message)
                    .map(|response| ReceivedFrom {
                        message: ReceivedMessage::QueryResponse(response),
                        from,
                    }),
            }
        });

        RpcTickReport {
            done_get_queries,
            done_put_queries,
            received_from,
        }
    }

    /// Send a request to the given address and return the transaction_id
    pub fn request(&mut self, address: SocketAddr, request: RequestSpecific) -> u16 {
        self.socket.request(address, request)
    }

    /// Send a response to the given address.
    pub fn response(
        &mut self,
        address: SocketAddr,
        transaction_id: u16,
        response: ResponseSpecific,
    ) {
        self.socket.response(address, transaction_id, response)
    }

    /// Send an error to the given address.
    pub fn error(&mut self, address: SocketAddr, transaction_id: u16, error: ErrorSpecific) {
        self.socket.error(address, transaction_id, error)
    }

    /// Store a value in the closest nodes, optionally trigger a lookup query if
    /// the cached closest_nodes aren't fresh enough.
    ///
    /// `salt` is only relevant for mutable values.
    pub fn put(
        &mut self,
        target: Id,
        request: PutRequestSpecific,
        sender: Option<Sender<Result<Id, PutError>>>,
    ) {
        if self.put_queries.contains_key(&target) {
            if let Some(sender) = sender {
                let _ = sender.send(Err(PutError::PutQueryIsInflight(target)));
            };

            debug!(?target, "Put query for the same target is already inflight");

            return;
        }

        let mut query = PutQuery::new(target, request.clone(), sender);

        if let Some(closest_nodes) = self
            .closest_nodes
            .get(&target)
            .filter(|nodes| !nodes.is_empty() && nodes.into_iter().any(|n| n.valid_token()))
        {
            query.start(&mut self.socket, closest_nodes.nodes())
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
                None,
            );
        };

        self.put_queries.insert(target, query);
    }

    /// Send a message to closer and closer nodes until we can't find any more nodes.
    ///
    /// Queries take few seconds to fully traverse the network, once it is done, it will be removed from
    /// self.queries. But until then, calling `rpc.query()` multiple times, will just add the
    /// sender to the query, send all the responses seen so far, as well as subsequent responses.
    ///
    /// Effectively, we are caching responses and backing off the network for the duration it takes
    /// to traverse it.
    ///
    /// `extra_nodes` option allows the query to visit specific nodes, that won't necessesarily be visited
    /// through the query otherwise.
    pub fn get(
        &mut self,
        target: Id,
        request: RequestTypeSpecific,
        sender: Option<ResponseSender>,
        extra_nodes: Option<Vec<SocketAddr>>,
    ) {
        // If query is still active, add the sender to it.
        if let Some(query) = self.queries.get_mut(&target) {
            if let Some(sender) = sender {
                query.add_sender(sender)
            };
            return;
        }

        let mut query = Query::new(
            target,
            RequestSpecific {
                requester_id: self.id,
                request_type: request,
            },
        );

        if let Some(sender) = sender {
            query.add_sender(sender)
        };

        // Seed the query either with the closest nodes from the routing table, or the
        // bootstrapping nodes if the closest nodes are not enough.

        let routing_table_closest = self.routing_table.closest(&target);

        // If we don't have enough or any closest nodes, call the bootstraping nodes.
        if routing_table_closest.is_empty() || routing_table_closest.len() < self.bootstrap.len() {
            for bootstrapping_node in self.bootstrap.clone() {
                query.visit(&mut self.socket, bootstrapping_node);
            }
        }

        if let Some(extra_nodes) = extra_nodes {
            for extra_node in extra_nodes {
                query.visit(&mut self.socket, extra_node)
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

    fn handle_response(&mut self, from: SocketAddr, message: &Message) -> Option<QueryResponse> {
        if message.read_only {
            return None;
        };

        // If the response looks like a Ping response, check StoreQueries for the transaction_id.
        if let Some(query) = self
            .put_queries
            .values_mut()
            .find(|query| query.inflight(message.transaction_id))
        {
            match &message.message_type {
                MessageType::Response(ResponseSpecific::Ping(_)) => {
                    // Mark storage at that node as a success.
                    query.success();
                }
                MessageType::Error(error) => query.error(error.clone()),
                _ => {}
            };

            return None;
        }

        // Get corresponing query for message.transaction_id
        if let Some(query) = self
            .queries
            .values_mut()
            .find(|query| query.inflight(message.transaction_id))
        {
            if let Some(nodes) = message.get_closer_nodes() {
                for node in nodes {
                    query.add_candidate(node);
                }
            }

            if let Some((responder_id, token)) = message.get_token() {
                query.add_responding_node(Node::new(responder_id, from).with_token(token.clone()));
            } else if let Some(responder_id) = message.get_author_id() {
                // update responding nodes even for FIND_NODE queries.
                query.add_responding_node(Node::new(responder_id, from));
            }

            let target = query.target();

            match &message.message_type {
                MessageType::Response(ResponseSpecific::GetPeers(GetPeersResponseArguments {
                    values,
                    ..
                })) => {
                    let response = Response::Peers(values.to_owned());
                    query.response(from, response.to_owned());

                    return Some(QueryResponse {
                        target,
                        response: QueryResponseSpecific::Value(response),
                    });
                }
                MessageType::Response(ResponseSpecific::GetImmutable(
                    GetImmutableResponseArguments {
                        v, responder_id, ..
                    },
                )) => {
                    if validate_immutable(v, &query.target()) {
                        let response = Response::Immutable(v.to_owned().into());

                        query.response(from, response.to_owned());

                        return Some(QueryResponse {
                            target,
                            response: QueryResponseSpecific::Value(response),
                        });
                    }

                    let target = query.target();
                    debug!(?v, ?target, ?responder_id, ?from, from_version = ?message.version, "Invalid immutable value");
                }
                MessageType::Response(ResponseSpecific::GetMutable(
                    GetMutableResponseArguments {
                        v,
                        seq,
                        sig,
                        k,
                        responder_id,
                        ..
                    },
                )) => {
                    let salt = match query.request.request_type.clone() {
                        RequestTypeSpecific::GetValue(args) => args.salt,
                        _ => None,
                    };
                    let target = query.target();

                    if let Ok(item) = MutableItem::from_dht_message(
                        &query.target(),
                        k,
                        v.to_owned().into(),
                        seq,
                        sig,
                        salt.to_owned(),
                        &None,
                    ) {
                        let response = Response::Mutable(item);

                        query.response(from, response.to_owned());

                        return Some(QueryResponse {
                            target,
                            response: QueryResponseSpecific::Value(response),
                        });
                    }

                    debug!(
                        ?v,
                        ?seq,
                        ?sig,
                        ?salt,
                        ?target,
                        ?from,
                        ?responder_id,
                        from_version = ?message.version,
                        "Invalid mutable record"
                    );
                }
                MessageType::Response(ResponseSpecific::NoMoreRecentValue(
                    NoMoreRecentValueResponseArguments {
                        seq, responder_id, ..
                    },
                )) => {
                    debug!(
                        target= ?query.target(),
                        salt= ?match query.request.request_type.clone() {
                            RequestTypeSpecific::GetValue(args) => args.salt,
                            _ => None,
                        },
                        ?seq,
                        ?from,
                        ?responder_id,
                        from_version = ?message.version,
                        "No more recent value"
                    );

                    return Some(QueryResponse {
                        target,
                        response: QueryResponseSpecific::Value(Response::NoMoreRecentValue(*seq)),
                    });
                }
                MessageType::Response(ResponseSpecific::NoValues(NoValuesResponseArguments {
                    responder_id,
                    ..
                })) => {
                    debug!(
                        target= ?query.target(),
                        salt= ?match query.request.request_type.clone() {
                            RequestTypeSpecific::GetValue(args) => args.salt,
                            _ => None,
                        },
                        ?from,
                        ?responder_id,
                        from_version = ?message.version,
                        "No values"
                    );
                }
                MessageType::Error(error) => {
                    debug!(?error, ?message, from_version = ?message.version, "Get query got error response");

                    return Some(QueryResponse {
                        target,
                        response: QueryResponseSpecific::Error(error.to_owned()),
                    });
                }
                // Ping response is already handled in add_node()
                // FindNode response is already handled in query.add_candidate()
                _ => {}
            };
        };

        None
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
        let node_id = self.id;
        debug!(?node_id, "Bootstraping the routing table");
        self.get(
            self.id,
            RequestTypeSpecific::FindNode(FindNodeRequestArguments { target: self.id }),
            None,
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

impl Drop for Rpc {
    fn drop(&mut self) {
        debug!("Dropped Mainline::Rpc");
    }
}

/// Any received message and done queries in the [Rpc::tick].
#[derive(Debug, Clone)]
pub struct RpcTickReport {
    /// All the [Id]s of the done [Rpc::get] queries.
    pub done_get_queries: Vec<Id>,
    /// All the [Id]s of the done [Rpc::put] queries.
    pub done_put_queries: Vec<Id>,
    /// The received message on this tick if any, and the SocketAddr of the sender.
    pub received_from: Option<ReceivedFrom>,
}

/// An incoming request or a query's response reported from [RpcTickReport::received_from]
#[derive(Debug, Clone)]
pub struct ReceivedFrom {
    /// The socket address of the sender.
    /// Useful to send a response for an incoming query using [Rpc::response]
    pub from: SocketAddr,
    pub message: ReceivedMessage,
}

#[derive(Debug, Clone)]
pub enum ReceivedMessage {
    /// An incoming request, as a tuple of the `transaction_id` and the RequestSpecific
    Request((u16, RequestSpecific)),
    /// A query response.
    QueryResponse(QueryResponse),
}

/// Target and payload of a response to a [Rpc::get] or [Rpc::put] query
#[derive(Debug, Clone)]
pub struct QueryResponse {
    pub target: Id,
    pub response: QueryResponseSpecific,
}

/// Rpc query resopnse; a value or an error.
#[derive(Debug, Clone)]
pub enum QueryResponseSpecific {
    /// A set of peers, immutable or mutable value response for a request
    Value(Response),
    /// An error response for a sent request
    Error(ErrorSpecific),
}

#[derive(Debug, Clone)]
pub enum Response {
    Peers(Vec<SocketAddr>),
    Immutable(Bytes),
    Mutable(MutableItem),
    NoMoreRecentValue(i64),
}

#[derive(Debug, Clone)]
pub enum ResponseSender {
    ClosestNodes(Sender<Vec<Node>>),
    Peers(Sender<Vec<SocketAddr>>),
    Mutable(Sender<MutableItem>),
    Immutable(Sender<Bytes>),
}
