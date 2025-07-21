//! K-RPC implementation.

mod closest_nodes;
pub(crate) mod config;
mod info;
mod iterative_query;
mod put_query;
pub(crate) mod server;
mod socket;

use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4, ToSocketAddrs};
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use lru::LruCache;
use tracing::{debug, error, info};

use iterative_query::IterativeQuery;
use put_query::PutQuery;

use crate::common::{
    validate_immutable, ErrorSpecific, FindNodeRequestArguments, GetImmutableResponseArguments,
    GetMutableResponseArguments, GetPeersResponseArguments, GetValueRequestArguments, Id, Message,
    MessageType, MutableItem, NoMoreRecentValueResponseArguments, NoValuesResponseArguments, Node,
    PutRequestSpecific, RequestSpecific, RequestTypeSpecific, ResponseSpecific, RoutingTable,
    MAX_BUCKET_SIZE_K,
};
use server::Server;

use self::messages::{GetPeersRequestArguments, PutMutableRequestArguments};
use server::ServerSettings;
use socket::KrpcSocket;

pub use crate::common::messages;
pub use closest_nodes::ClosestNodes;
pub use info::Info;
pub use iterative_query::GetRequestSpecific;
pub use put_query::{ConcurrencyError, PutError, PutQueryError};
pub use socket::DEFAULT_REQUEST_TIMEOUT;

pub const DEFAULT_BOOTSTRAP_NODES: [&str; 4] = [
    "router.bittorrent.com:6881",
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401",
    "relay.pkarr.org:6881",
];

const REFRESH_TABLE_INTERVAL: Duration = Duration::from_secs(15 * 60);
const PING_TABLE_INTERVAL: Duration = Duration::from_secs(5 * 60);

const MAX_CACHED_ITERATIVE_QUERIES: usize = 1000;

#[derive(Debug)]
/// Internal Rpc called in the Dht thread loop, useful to create your own actor setup.
pub struct Rpc {
    // Options
    bootstrap: Box<[SocketAddrV4]>,

    socket: KrpcSocket,

    // Routing
    /// Closest nodes to this node
    routing_table: RoutingTable,
    /// Last time we refreshed the routing table with a find_node query.
    last_table_refresh: Instant,
    /// Last time we pinged nodes in the routing table.
    last_table_ping: Instant,
    /// Closest responding nodes to specific target
    ///
    /// as well as the:
    /// 1. dht size estimate based on closest claimed nodes,
    /// 2. dht size estimate based on closest responding nodes.
    /// 3. number of subnets with unique 6 bits prefix in ipv4
    cached_iterative_queries: LruCache<Id, CachedIterativeQuery>,

    // Active IterativeQueries
    iterative_queries: HashMap<Id, IterativeQuery>,
    /// Put queries are special, since they have to wait for a corresponding
    /// get query to finish, update the closest_nodes, then `query_all` these.
    put_queries: HashMap<Id, PutQuery>,

    /// Sum of Dht size estimates from closest nodes from get queries.
    dht_size_estimates_sum: f64,

    /// Sum of Dht size estimates from closest _responding_ nodes from get queries.
    responders_based_dht_size_estimates_sum: f64,
    responders_based_dht_size_estimates_count: usize,

    /// Sum of the number of subnets with 6 bits prefix in the closest nodes ipv4
    subnets_sum: usize,

    server: Server,

    public_address: Option<SocketAddrV4>,
    firewalled: bool,
}

impl Rpc {
    /// Create a new Rpc
    pub fn new(config: config::Config) -> Result<Self, std::io::Error> {
        let id = if let Some(ip) = config.public_ip {
            Id::from_ip(ip.into())
        } else {
            Id::random()
        };

        let socket = KrpcSocket::new(&config)?;

        Ok(Rpc {
            bootstrap: config
                .bootstrap
                .unwrap_or(to_socket_address(&DEFAULT_BOOTSTRAP_NODES))
                .into(),
            socket,

            routing_table: RoutingTable::new(id),
            iterative_queries: HashMap::new(),
            put_queries: HashMap::new(),

            cached_iterative_queries: LruCache::new(
                NonZeroUsize::new(MAX_CACHED_ITERATIVE_QUERIES)
                    .expect("MAX_CACHED_BUCKETS is NonZeroUsize"),
            ),

            last_table_refresh: Instant::now(),
            last_table_ping: Instant::now(),

            dht_size_estimates_sum: 0.0,
            responders_based_dht_size_estimates_count: 0,

            // Don't store to too many nodes just because you are in a cold start.
            responders_based_dht_size_estimates_sum: 1_000_000.0,
            subnets_sum: 20,

            server: Server::new(config.server_settings),

            public_address: None,
            firewalled: true,
        })
    }

    // === Getters ===

    /// Returns the node's Id
    pub fn id(&self) -> &Id {
        self.routing_table.id()
    }

    /// Returns the address the server is listening to.
    #[inline]
    pub fn local_addr(&self) -> SocketAddrV4 {
        self.socket.local_addr()
    }

    /// Returns the best guess for this node's Public address.
    ///
    /// If [crate::DhtBuilder::public_ip] was set, this is what will be returned
    /// (plus the local port), otherwise it will rely on consensus from
    /// responding nodes voting on our public IP and port.
    pub fn public_address(&self) -> Option<SocketAddrV4> {
        self.public_address
    }

    /// Returns `true` if we can't confirm that [Self::public_address] is publicly addressable.
    ///
    /// If this node is firewalled, it won't switch to server mode if it is in adaptive mode,
    /// but if [crate::DhtBuilder::server_mode] was set to true, then whether or not this node is firewalled
    /// won't matter.
    pub fn firewalled(&self) -> bool {
        self.firewalled
    }

    /// Returns whether or not this node is running in server mode.
    pub fn server_mode(&self) -> bool {
        self.socket.server_mode
    }

    pub fn routing_table(&self) -> &RoutingTable {
        &self.routing_table
    }

    /// Returns:
    ///  1. Normal Dht size estimate based on all closer `nodes` in query responses.
    ///  2. Standard deviaiton as a function of the number of samples used in this estimate.
    ///
    /// [Read more](https://github.com/pubky/mainline/blob/main/docs/dht_size_estimate.md)
    pub fn dht_size_estimate(&self) -> (usize, f64) {
        let normal =
            self.dht_size_estimates_sum as usize / self.cached_iterative_queries.len().max(1);

        // See https://github.com/pubky/mainline/blob/main/docs/standard-deviation-vs-lookups.png
        let std_dev = 0.281 * (self.cached_iterative_queries.len() as f64).powf(-0.529);

        (normal, std_dev)
    }

    /// Returns a thread safe and lightweight summary of this node's
    /// information and statistics.
    pub fn info(&self) -> Info {
        Info::from(self)
    }

    // === Public Methods ===

    /// Advance the inflight queries, receive incoming requests,
    /// maintain the routing table, and everything else that needs
    /// to happen at every tick.
    pub fn tick(&mut self) -> RpcTickReport {
        let mut done_get_queries = Vec::with_capacity(self.iterative_queries.len());
        let mut done_put_queries = Vec::with_capacity(self.put_queries.len());

        // === Tick Queries ===

        for (id, query) in self.put_queries.iter_mut() {
            match query.tick(&self.socket) {
                Ok(done) => {
                    if done {
                        done_put_queries.push((*id, None));
                    }
                }
                Err(error) => done_put_queries.push((*id, Some(error))),
            };
        }

        let self_id = *self.id();
        let table_size = self.routing_table.size();

        let responders_based_dht_size_estimate = self.responders_based_dht_size_estimate();
        let average_subnets = self.average_subnets();

        for (id, query) in self.iterative_queries.iter_mut() {
            let is_done = query.tick(&mut self.socket);

            if is_done {
                let closest_nodes =
                    if let RequestTypeSpecific::FindNode(_) = query.request.request_type {
                        if *id == self_id {
                            if table_size == 0 {
                                error!("Could not bootstrap the routing table");
                            } else {
                                debug!(?self_id, table_size, "Populated the routing table");
                            }
                        };

                        query
                            .closest()
                            .nodes()
                            .iter()
                            .take(MAX_BUCKET_SIZE_K)
                            .cloned()
                            .collect::<Box<[_]>>()
                    } else {
                        query
                            .responders()
                            .take_until_secure(responders_based_dht_size_estimate, average_subnets)
                            .to_vec()
                            .into_boxed_slice()
                    };

                done_get_queries.push((*id, closest_nodes));
            };
        }

        // === Cleanup done queries ===

        // Has to happen _before_ `self.socket.recv_from()`.
        for (id, closest_nodes) in &done_get_queries {
            if let Some(query) = self.iterative_queries.remove(id) {
                self.update_address_votes_from_iterative_query(&query);
                self.cache_iterative_query(&query, closest_nodes);

                // Only for get queries, not find node.
                if !matches!(query.request.request_type, RequestTypeSpecific::FindNode(_)) {
                    if let Some(put_query) = self.put_queries.get_mut(id) {
                        if !put_query.started() {
                            if let Err(error) = put_query.start(&mut self.socket, closest_nodes) {
                                done_put_queries.push((*id, Some(error)))
                            }
                        }
                    }
                }
            };
        }

        for (id, _) in &done_put_queries {
            self.put_queries.remove(id);
        }

        // === Periodic node maintaenance ===
        self.periodic_node_maintaenance();

        // Handle new incoming message
        let new_query_response = self
            .socket
            .recv_from()
            .and_then(|(message, from)| match message.message_type {
                MessageType::Request(request_specific) => {
                    self.handle_request(from, message.transaction_id, request_specific);

                    None
                }
                _ => self.handle_response(from, message),
            });

        RpcTickReport {
            done_get_queries,
            done_put_queries,
            new_query_response,
        }
    }

    /// Send a request to the given address and return the transaction_id
    pub fn request(&mut self, address: SocketAddrV4, request: RequestSpecific) -> u32 {
        self.socket.request(address, request)
    }

    /// Send a response to the given address.
    pub fn response(
        &mut self,
        address: SocketAddrV4,
        transaction_id: u32,
        response: ResponseSpecific,
    ) {
        self.socket.response(address, transaction_id, response)
    }

    /// Send an error to the given address.
    pub fn error(&mut self, address: SocketAddrV4, transaction_id: u32, error: ErrorSpecific) {
        self.socket.error(address, transaction_id, error)
    }

    /// Store a value in the closest nodes, optionally trigger a lookup query if
    /// the cached closest_nodes aren't fresh enough.
    ///
    /// - `request`: the put request.
    pub fn put(
        &mut self,
        request: PutRequestSpecific,
        extra_nodes: Option<Box<[Node]>>,
    ) -> Result<(), PutError> {
        let target = *request.target();

        if let PutRequestSpecific::PutMutable(PutMutableRequestArguments {
            sig, cas, seq, ..
        }) = &request
        {
            if let Some(PutRequestSpecific::PutMutable(inflight_request)) = self
                .put_queries
                .get(&target)
                .map(|existing| &existing.request)
            {
                debug!(?inflight_request, ?request, "Possible conflict risk");

                if *sig == inflight_request.sig {
                    // Noop, the inflight query is sufficient.
                    return Ok(());
                } else if *seq < inflight_request.seq {
                    return Err(ConcurrencyError::NotMostRecent)?;
                } else if let Some(cas) = cas {
                    if *cas == inflight_request.seq {
                        // The user is aware of the inflight query and whiches to overrides it.
                        //
                        // Remove the inflight request, and create a new one.
                        self.put_queries.remove(&target);
                    } else {
                        return Err(ConcurrencyError::CasFailed)?;
                    }
                } else {
                    return Err(ConcurrencyError::ConflictRisk)?;
                };
            };
        }

        let mut query = PutQuery::new(target, request.clone(), extra_nodes);

        if let Some(closest_nodes) = self
            .cached_iterative_queries
            .get(&target)
            .map(|cached| cached.closest_responding_nodes.clone())
            .filter(|closest_nodes| {
                !closest_nodes.is_empty() && closest_nodes.iter().any(|n| n.valid_token())
            })
        {
            query.start(&mut self.socket, &closest_nodes)?
        } else {
            let salt = match request {
                PutRequestSpecific::PutMutable(args) => args.salt,
                _ => None,
            };

            self.get(
                GetRequestSpecific::GetValue(GetValueRequestArguments {
                    target,
                    seq: None,
                    salt,
                }),
                None,
            );
        };

        self.put_queries.insert(target, query);

        Ok(())
    }

    /// Send a message to closer and closer nodes until we can't find any more nodes.
    ///
    /// Queries take few seconds to fully traverse the network, once it is done, it will be removed from
    /// self.iterative_queries. But until then, calling [Rpc::get] multiple times, will just return the list
    /// of responses seen so far.
    ///
    /// Subsequent responses can be obtained from the [RpcTickReport::new_query_response] you get after calling [Rpc::tick].
    ///
    /// Effectively, we are caching responses and backing off the network for the duration it takes
    /// to traverse it.
    ///
    /// - `request` [RequestTypeSpecific], except [RequestTypeSpecific::Ping] and
    ///   [RequestTypeSpecific::Put] which will be ignored.
    /// - `extra_nodes` option allows the query to visit specific nodes, that won't necessesarily be visited
    ///   through the query otherwise.
    pub fn get(
        &mut self,
        request: GetRequestSpecific,
        extra_nodes: Option<&[SocketAddrV4]>,
    ) -> Option<Vec<Response>> {
        let target = match request {
            GetRequestSpecific::FindNode(FindNodeRequestArguments { target }) => target,
            GetRequestSpecific::GetPeers(GetPeersRequestArguments { info_hash, .. }) => info_hash,
            GetRequestSpecific::GetValue(GetValueRequestArguments { target, .. }) => target,
        };

        let response_from_inflight_put_mutable_request =
            self.put_queries.get(&target).and_then(|existing| {
                if let PutRequestSpecific::PutMutable(request) = &existing.request {
                    Some(Response::Mutable(request.clone().into()))
                } else {
                    None
                }
            });

        // If query is still active, no need to create a new one.
        if let Some(query) = self.iterative_queries.get(&target) {
            let mut responses = query.responses().to_vec();

            if let Some(response) = response_from_inflight_put_mutable_request {
                responses.push(response);
            }

            return Some(responses);
        }

        let node_id = self.routing_table.id();

        if target == *node_id {
            debug!(?node_id, "Bootstrapping the routing table");
        }

        let mut query = IterativeQuery::new(*self.id(), target, request);

        // Seed the query either with the closest nodes from the routing table, or the
        // bootstrapping nodes if the closest nodes are not enough.

        let routing_table_closest = self.routing_table.closest_secure(
            target,
            self.responders_based_dht_size_estimate(),
            self.average_subnets(),
        );

        // If we don't have enough or any closest nodes, call the bootstrapping nodes.
        if routing_table_closest.is_empty() || routing_table_closest.len() < self.bootstrap.len() {
            for bootstrapping_node in self.bootstrap.clone() {
                query.visit(&mut self.socket, bootstrapping_node);
            }
        }

        if let Some(extra_nodes) = extra_nodes {
            for extra_node in extra_nodes {
                query.visit(&mut self.socket, *extra_node)
            }
        }

        // Seed this query with the closest nodes we know about.
        for node in routing_table_closest {
            query.add_candidate(node)
        }

        if let Some(CachedIterativeQuery {
            closest_responding_nodes,
            ..
        }) = self.cached_iterative_queries.get(&target)
        {
            for node in closest_responding_nodes {
                query.add_candidate(node.clone())
            }
        }

        // After adding the nodes, we need to start the query.
        query.start(&mut self.socket);

        self.iterative_queries.insert(target, query);

        // If there is an inflight PutQuery for mutable item return its value
        if let Some(response) = response_from_inflight_put_mutable_request {
            return Some(vec![response]);
        }

        None
    }

    // === Private Methods ===

    fn handle_request(
        &mut self,
        from: SocketAddrV4,
        transaction_id: u32,
        request_specific: RequestSpecific,
    ) {
        // By default we only add nodes that responds to our requests.
        //
        // This is the only exception; the first node creating the DHT,
        // without this exception, the bootstrapping node's routing table
        // will never be populated.
        if self.bootstrap.is_empty() {
            if let RequestTypeSpecific::FindNode(param) = &request_specific.request_type {
                self.routing_table.add(Node::new(param.target, from));
            }
        }

        let is_ping = matches!(request_specific.request_type, RequestTypeSpecific::Ping);

        if self.server_mode() {
            let server = &mut self.server;

            match server.handle_request(&self.routing_table, from, request_specific) {
                Some(MessageType::Error(error)) => {
                    self.error(from, transaction_id, error);
                }
                Some(MessageType::Response(response)) => {
                    self.response(from, transaction_id, response);
                }
                _ => {}
            };
        }

        if let Some(our_address) = self.public_address {
            if from == our_address && is_ping {
                self.firewalled = false;

                let ipv4 = our_address.ip();

                // Restarting our routing table with new secure Id if necessary.
                if !self.id().is_valid_for_ip(*ipv4) {
                    let new_id = Id::from_ipv4(*ipv4);

                    info!(
                        "Our current id {} is not valid for adrsess {}. Using new id {}",
                        self.id(),
                        our_address,
                        new_id
                    );

                    self.get(
                        GetRequestSpecific::FindNode(FindNodeRequestArguments { target: new_id }),
                        None,
                    );

                    self.routing_table = RoutingTable::new(new_id);
                }
            }
        }
    }

    fn handle_response(&mut self, from: SocketAddrV4, message: Message) -> Option<(Id, Response)> {
        // If someone claims to be readonly, then let's not store anything even if they respond.
        if message.read_only {
            return None;
        };

        // If the response looks like a Ping response, check StoreQueries for the transaction_id.
        if let Some(query) = self
            .put_queries
            .values_mut()
            .find(|query| query.inflight(message.transaction_id))
        {
            match message.message_type {
                MessageType::Response(ResponseSpecific::Ping(_)) => {
                    // Mark storage at that node as a success.
                    query.success();
                }
                MessageType::Error(error) => query.error(error),
                _ => {}
            };

            return None;
        }

        let mut should_add_node = false;
        let author_id = message.get_author_id();
        let from_version = message.version.to_owned();

        // Get corresponding query for message.transaction_id
        if let Some(query) = self
            .iterative_queries
            .values_mut()
            .find(|query| query.inflight(message.transaction_id))
        {
            // KrpcSocket would not give us a response from the wrong address for the transaction_id
            should_add_node = true;

            if let Some(nodes) = message.get_closer_nodes() {
                for node in nodes {
                    query.add_candidate(node.clone());
                }
            }

            if let Some((responder_id, token)) = message.get_token() {
                query.add_responding_node(Node::new_with_token(responder_id, from, token.into()));
            }

            if let Some(proposed_ip) = message.requester_ip {
                query.add_address_vote(proposed_ip);
            }

            let target = query.target();

            match message.message_type {
                MessageType::Response(ResponseSpecific::GetPeers(GetPeersResponseArguments {
                    values,
                    ..
                })) => {
                    let response = Response::Peers(values);
                    query.response(from, response.clone());

                    return Some((target, response));
                }
                MessageType::Response(ResponseSpecific::GetImmutable(
                    GetImmutableResponseArguments {
                        v, responder_id, ..
                    },
                )) => {
                    if validate_immutable(&v, query.target()) {
                        let response = Response::Immutable(v);
                        query.response(from, response.clone());

                        return Some((target, response));
                    }

                    let target = query.target();
                    debug!(
                        ?v,
                        ?target,
                        ?responder_id,
                        ?from,
                        ?from_version,
                        "Invalid immutable value"
                    );
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

                    match MutableItem::from_dht_message(query.target(), &k, v, seq, &sig, salt) {
                        Ok(item) => {
                            let response = Response::Mutable(item);
                            query.response(from, response.clone());

                            return Some((target, response));
                        }
                        Err(error) => {
                            debug!(
                                ?error,
                                ?from,
                                ?responder_id,
                                ?from_version,
                                "Invalid mutable record"
                            );
                        }
                    }
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
                        ?from_version,
                        "No more recent value"
                    );
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
                        ?from_version ,
                        "No values"
                    );
                }
                MessageType::Error(error) => {
                    debug!(?error, ?from_version, "Get query got error response");
                }
                // Ping response is already handled in add_node()
                // FindNode response is already handled in query.add_candidate()
                // Requests are handled elsewhere
                MessageType::Response(ResponseSpecific::Ping(_))
                | MessageType::Response(ResponseSpecific::FindNode(_))
                | MessageType::Request(_) => {}
            };
        };

        if should_add_node {
            // Add a node to our routing table on any expected incoming response.

            if let Some(id) = author_id {
                self.routing_table.add(Node::new(id, from));
            }
        }

        None
    }

    fn periodic_node_maintaenance(&mut self) {
        // Bootstrap if necessary
        if self.routing_table.is_empty() {
            self.populate();
        }

        // Every 15 minutes refresh the routing table.
        if self.last_table_refresh.elapsed() > REFRESH_TABLE_INTERVAL {
            self.last_table_refresh = Instant::now();

            if !self.server_mode() && !self.firewalled() {
                info!("Adaptive mode: have been running long enough (not firewalled), switching to server mode");

                self.socket.server_mode = true;
            }

            self.populate();
        }

        if self.last_table_ping.elapsed() > PING_TABLE_INTERVAL {
            self.last_table_ping = Instant::now();

            let mut to_remove = Vec::with_capacity(self.routing_table.size());
            let mut to_ping = Vec::with_capacity(self.routing_table.size());

            for node in self.routing_table.nodes() {
                if node.is_stale() {
                    to_remove.push(*node.id())
                } else if node.should_ping() {
                    to_ping.push(node.address())
                }
            }

            for id in to_remove {
                self.routing_table.remove(&id);
            }

            for address in to_ping {
                self.ping(address);
            }
        }
    }

    /// Ping bootstrap nodes, add them to the routing table with closest query.
    fn populate(&mut self) {
        if self.bootstrap.is_empty() {
            return;
        }

        self.get(
            GetRequestSpecific::FindNode(FindNodeRequestArguments { target: *self.id() }),
            None,
        );
    }

    fn ping(&mut self, address: SocketAddrV4) {
        self.socket.request(
            address,
            RequestSpecific {
                requester_id: *self.id(),
                request_type: RequestTypeSpecific::Ping,
            },
        );
    }

    fn update_address_votes_from_iterative_query(&mut self, query: &IterativeQuery) {
        if let Some(new_address) = query.best_address() {
            if self.public_address.is_none()
                || new_address
                    != self
                        .public_address
                        .expect("self.public_address is not None")
            {
                debug!(
                    ?new_address,
                    "Query responses suggest a different public_address, trying to confirm.."
                );

                self.firewalled = true;
                self.ping(new_address);
            }

            self.public_address = Some(new_address)
        }
    }

    fn cache_iterative_query(&mut self, query: &IterativeQuery, closest_responding_nodes: &[Node]) {
        if self.cached_iterative_queries.len() >= MAX_CACHED_ITERATIVE_QUERIES {
            let q = self.cached_iterative_queries.pop_lru();
            self.decrement_cached_iterative_query_stats(q.map(|q| q.1));
        }

        let closest = query.closest();
        let responders = query.responders();

        if closest.nodes().is_empty() {
            // We are clearly offline.
            return;
        }

        let dht_size_estimate = closest.dht_size_estimate();
        let responders_dht_size_estimate = responders.dht_size_estimate();
        let subnets_count = closest.subnets_count();

        let previous = self.cached_iterative_queries.put(
            query.target(),
            CachedIterativeQuery {
                closest_responding_nodes: closest_responding_nodes.into(),
                dht_size_estimate,
                responders_dht_size_estimate,
                subnets: subnets_count,

                is_find_node: matches!(
                    query.request.request_type,
                    RequestTypeSpecific::FindNode(_)
                ),
            },
        );

        self.decrement_cached_iterative_query_stats(previous);

        self.dht_size_estimates_sum += dht_size_estimate;
        self.responders_based_dht_size_estimates_sum += responders_dht_size_estimate;
        self.subnets_sum += subnets_count as usize;
        self.responders_based_dht_size_estimates_count += 1;
    }

    fn responders_based_dht_size_estimate(&self) -> usize {
        self.responders_based_dht_size_estimates_sum as usize
            / self.responders_based_dht_size_estimates_count.max(1)
    }

    fn average_subnets(&self) -> usize {
        self.subnets_sum / self.cached_iterative_queries.len().max(1)
    }

    fn decrement_cached_iterative_query_stats(&mut self, query: Option<CachedIterativeQuery>) {
        if let Some(CachedIterativeQuery {
            dht_size_estimate,
            responders_dht_size_estimate,
            subnets,
            is_find_node,
            ..
        }) = query
        {
            self.dht_size_estimates_sum -= dht_size_estimate;
            self.responders_based_dht_size_estimates_sum -= responders_dht_size_estimate;
            self.subnets_sum -= subnets as usize;

            if !is_find_node {
                self.responders_based_dht_size_estimates_count -= 1;
            }
        };
    }
}

struct CachedIterativeQuery {
    closest_responding_nodes: Box<[Node]>,
    dht_size_estimate: f64,
    responders_dht_size_estimate: f64,
    subnets: u8,

    /// Keeping track of find_node queries, because they shouldn't
    /// be counted in `responders_based_dht_size_estimates_count`
    is_find_node: bool,
}

/// State change after a call to [Rpc::tick], including
/// done PUT, GET, and FIND_NODE queries, as well as any
/// incoming value response for any GET query.
#[derive(Debug, Clone)]
pub struct RpcTickReport {
    /// All the [Id]s of the done [Rpc::get] queries.
    pub done_get_queries: Vec<(Id, Box<[Node]>)>,
    /// All the [Id]s of the done [Rpc::put] queries,
    /// and optional [PutError] if the query failed.
    pub done_put_queries: Vec<(Id, Option<PutError>)>,
    /// Received GET query response.
    pub new_query_response: Option<(Id, Response)>,
}

#[derive(Debug, Clone)]
pub enum Response {
    Peers(Vec<SocketAddrV4>),
    Immutable(Box<[u8]>),
    Mutable(MutableItem),
}

pub(crate) fn to_socket_address<T: ToSocketAddrs>(bootstrap: &[T]) -> Vec<SocketAddrV4> {
    bootstrap
        .iter()
        .flat_map(|s| {
            s.to_socket_addrs().map(|addrs| {
                addrs
                    .filter_map(|addr| match addr {
                        SocketAddr::V4(addr_v4) => Some(addr_v4),
                        _ => None,
                    })
                    .collect::<Box<[_]>>()
            })
        })
        .flatten()
        .collect()
}
