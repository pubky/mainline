//! Actor implementation - I/O orchestration layer for the DHT.

pub(crate) mod config;
mod handle_request;
mod handle_response;
mod info;
pub(crate) mod socket;

use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4, ToSocketAddrs};

use tracing::{debug, error, info};

use crate::core::iterative_query::IterativeQuery;
use crate::core::put_query::PutQuery;

use crate::common::{
    ErrorSpecific, FindNodeRequestArguments, GetValueRequestArguments, Id, MessageType,
    MutableItem, Node, PutRequestSpecific, RequestSpecific, RequestTypeSpecific, ResponseSpecific,
    RoutingTable, MAX_BUCKET_SIZE_K,
};
use crate::core::routing_maintenance::RoutingMaintenance;
use crate::core::server::Server;
use crate::core::statistics::DhtStatistics;

use self::messages::{GetPeersRequestArguments, PutMutableRequestArguments};
use socket::KrpcSocket;

pub use crate::common::messages;
pub use crate::core::iterative_query::GetRequestSpecific;
pub use crate::core::put_query::{ConcurrencyError, PutError, PutQueryError};
pub use crate::core::server::{
    RequestFilter, ServerSettings, MAX_INFO_HASHES, MAX_PEERS, MAX_VALUES,
};
pub use info::Info;
pub use socket::DEFAULT_REQUEST_TIMEOUT;

/// Result of `tick_get_queries`: completed queries and whether self-findnode finished.
type GetQueriesResult = (Vec<(Id, Box<[Node]>)>, bool);

pub const DEFAULT_BOOTSTRAP_NODES: [&str; 4] = [
    "router.bittorrent.com:6881",
    "dht.transmissionbt.com:6881",
    "dht.libtorrent.org:25401",
    "relay.pkarr.org:6881",
];

#[derive(Debug)]
/// Internal Actor called in the Dht thread loop, useful to create your own actor setup.
pub struct Actor {
    // Options
    bootstrap: Box<[SocketAddrV4]>,

    socket: KrpcSocket,

    // Routing
    /// Closest nodes to this node
    routing_table: RoutingTable,
    /// Routing table maintenance (refresh/ping timing)
    maintenance: RoutingMaintenance,

    // DHT statistics and cached queries
    statistics: DhtStatistics,

    // Active IterativeQueries
    iterative_queries: HashMap<Id, IterativeQuery>,
    /// Put queries are special, since they have to wait for a corresponding
    /// get query to finish, update the closest_nodes, then `query_all` these.
    put_queries: HashMap<Id, PutQuery>,

    server: Server,

    public_address: Option<SocketAddrV4>,
    firewalled: bool,
}

impl Actor {
    /// Creates a new Actor. Does not perform network I/O; call [`Actor::tick`] to
    /// bootstrap and run scheduled maintenance.
    pub fn new(config: config::Config) -> Result<Self, std::io::Error> {
        let id = if let Some(ip) = config.public_ip {
            Id::from_ip(ip.into())
        } else {
            Id::random()
        };

        let socket = KrpcSocket::new(&config)?;

        Ok(Actor {
            bootstrap: config
                .bootstrap
                .unwrap_or_else(|| to_socket_address(&DEFAULT_BOOTSTRAP_NODES))
                .into(),
            socket,

            routing_table: RoutingTable::new(id),
            maintenance: RoutingMaintenance::new(),
            statistics: DhtStatistics::new(),

            iterative_queries: HashMap::new(),
            put_queries: HashMap::new(),

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

    pub fn routing_table_mut(&mut self) -> &mut RoutingTable {
        &mut self.routing_table
    }

    /// Returns:
    ///  1. Normal Dht size estimate based on all closer `nodes` in query responses.
    ///  2. Standard deviaiton as a function of the number of samples used in this estimate.
    ///
    /// [Read more](https://github.com/pubky/mainline/blob/main/docs/dht_size_estimate.md)
    pub fn dht_size_estimate(&self) -> (usize, f64) {
        self.statistics.dht_size_estimate()
    }

    /// Returns a thread safe and lightweight summary of this node's
    /// information and statistics.
    pub fn info(&self) -> Info {
        Info::from(self)
    }

    // === Public Methods ===

    /// Advances routing-table maintenance and in-flight queries by one step.
    ///
    /// Call periodically; delays degrade query completion and routing table quality.
    pub fn tick(&mut self) -> RpcTickReport {
        let mut done_put_queries = self.tick_put_queries();

        let (done_get_queries, finished_self_findnode) = self.tick_get_queries();

        self.cleanup_done_queries(&done_get_queries, &mut done_put_queries);

        self.periodic_node_maintenance();

        let new_query_response = self.handle_message();

        if finished_self_findnode {
            self.log_bootstrap(self.id());
        }

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

        // Extract salt before moving request (only clone small salt, not entire request)
        let salt = match &request {
            PutRequestSpecific::PutMutable(args) => args.salt.clone(),
            _ => None,
        };

        let mut query = PutQuery::new(target, request, extra_nodes);

        if let Some(cached_nodes) = self.statistics.get_cached_query(&target) {
            let closest_nodes: Box<[Node]> = cached_nodes.into();
            if !closest_nodes.is_empty() && closest_nodes.iter().any(|n| n.valid_token()) {
                query.start(&mut self.socket, &closest_nodes)?;
                self.put_queries.insert(target, query);
                return Ok(());
            }
        }

        // No cached nodes with valid tokens, need to do a GET first
        self.get(
            GetRequestSpecific::GetValue(GetValueRequestArguments {
                target,
                seq: None,
                salt,
            }),
            None,
        );

        self.put_queries.insert(target, query);

        Ok(())
    }

    /// Start an iterative lookup toward `target`. While the query is in flight,
    /// repeated calls return cached responses. New responses arrive via
    /// [`RpcTickReport::new_query_response`] after [`Actor::tick`].
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

        if let Some(closest_responding_nodes) = self.statistics.get_cached_query(&target) {
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

    /// Run periodic routing-table maintenance (purge, ping, repopulate, server-mode check).
    fn periodic_node_maintenance(&mut self) {
        let decisions = self
            .maintenance
            .periodic_maintenance_decisions(&self.routing_table);

        if decisions.should_switch_to_server {
            self.try_switching_to_server_mode();
        }

        if decisions.should_ping {
            self.purge_nodes(&decisions.nodes_to_purge);
            self.ping_nodes(&decisions.nodes_to_ping);

            if !decisions.nodes_to_purge.is_empty() || !decisions.nodes_to_ping.is_empty() {
                debug!(
                    removed = decisions.nodes_to_purge.len(),
                    pinged = decisions.nodes_to_ping.len(),
                    "Node maintenance executed"
                );
            }
        }

        if decisions.should_populate {
            self.populate();
        }
    }

    /// Switch to server mode if not already active and not firewalled.
    fn try_switching_to_server_mode(&mut self) {
        if !self.server_mode() && !self.firewalled() {
            info!("Adaptive mode: have been running long enough (not firewalled), switching to server mode");
            self.socket.server_mode = true;
        }
    }

    /// Remove nodes from the routing table.
    fn purge_nodes(&mut self, ids: &[Id]) {
        for id in ids {
            self.routing_table.remove(id);
        }
    }

    /// Ping nodes.
    fn ping_nodes(&mut self, addrs: &[SocketAddrV4]) {
        for address in addrs {
            self.ping(*address);
        }
    }

    /// Populate routing table by asking bootstrap nodes to find ourselves,
    /// Response will allow to add closest nodes candidates to routing table.
    fn populate(&mut self) {
        if self.bootstrap.is_empty() {
            return;
        }

        self.get(
            GetRequestSpecific::FindNode(FindNodeRequestArguments { target: *self.id() }),
            None,
        );
    }

    /// Send a ping request to a node.
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
        let Some(new_address) = query.best_address() else {
            return;
        };

        let needs_confirm = match self.public_address {
            None => true,
            Some(current) => current != new_address,
        };

        if needs_confirm {
            debug!(
                ?new_address,
                "Query responses suggest a different public_address, trying to confirm.."
            );

            self.firewalled = true;
            self.ping(new_address);
        }

        self.public_address = Some(new_address);
    }

    fn cache_iterative_query(&mut self, query: &IterativeQuery, closest_responding_nodes: &[Node]) {
        self.statistics.cache_query(query, closest_responding_nodes);
    }

    fn responders_based_dht_size_estimate(&self) -> usize {
        self.statistics.responders_based_dht_size_estimate()
    }

    fn average_subnets(&self) -> usize {
        self.statistics.average_subnets()
    }

    // === tick() helpers ===

    /// Advance all PUT queries, return done ones.
    fn tick_put_queries(&mut self) -> Vec<(Id, Option<PutError>)> {
        let mut done_put_queries = Vec::with_capacity(self.put_queries.len());

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

        done_put_queries
    }

    /// Advance all GET/FIND_NODE queries, return done ones and whether table refresh/find_node to self is finished.
    fn tick_get_queries(&mut self) -> GetQueriesResult {
        let self_id = *self.id();
        let responders_based_dht_size_estimate = self.responders_based_dht_size_estimate();
        let average_subnets = self.average_subnets();

        let mut done_get_queries = Vec::with_capacity(self.iterative_queries.len());
        let mut finished_self_findnode = false;

        for (id, query) in self.iterative_queries.iter_mut() {
            if !query.tick(&mut self.socket) {
                continue;
            }

            let closest_nodes = if let RequestTypeSpecific::FindNode(_) = query.request.request_type
            {
                finished_self_findnode = *id == self_id;

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
        }

        (done_get_queries, finished_self_findnode)
    }

    /// Remove completed GET and PUT queries from internal state.
    fn cleanup_done_queries(
        &mut self,
        done_get: &[(Id, Box<[Node]>)],
        done_put: &mut Vec<(Id, Option<PutError>)>,
    ) {
        // Has to happen _before_ `self.socket.recv_from()`.
        for (id, closest_nodes) in done_get {
            let query = match self.iterative_queries.remove(id) {
                Some(query) => query,
                None => continue,
            };

            self.update_address_votes_from_iterative_query(&query);
            self.cache_iterative_query(&query, closest_nodes);

            // Only for get queries, not find node.
            if matches!(query.request.request_type, RequestTypeSpecific::FindNode(_)) {
                continue;
            }

            let put_query = match self.put_queries.get_mut(id) {
                Some(put_query) => put_query,
                None => continue,
            };

            if put_query.started() {
                continue;
            }

            if let Err(error) = put_query.start(&mut self.socket, closest_nodes) {
                done_put.push((*id, Some(error)))
            }
        }

        for (id, _) in done_put.iter() {
            self.put_queries.remove(id);
        }
    }

    /// Handle one incoming message, either a request or a response message. One message per tick.
    fn handle_message(&mut self) -> Option<(Id, Response)> {
        self.socket
            .recv_from()
            .and_then(|(message, from)| match message.message_type {
                MessageType::Request(request_specific) => {
                    self.handle_request(from, message.transaction_id, request_specific);
                    None
                }
                _ => self.handle_response(from, message),
            })
    }

    /// Check if routing table is empty and log an error if so.
    fn log_bootstrap(&self, self_id: &Id) {
        let table_size = self.routing_table.size();
        if table_size == 0 {
            error!("Could not bootstrap the routing table");
        } else {
            debug!(?self_id, table_size, "Populated the routing table");
        }
    }
}

/// Results from a single [`Actor::tick`] call.
#[derive(Debug, Clone)]
pub struct RpcTickReport {
    /// Completed GET queries with their closest nodes.
    pub done_get_queries: Vec<(Id, Box<[Node]>)>,
    /// Completed PUT queries; `Some(err)` on failure.
    pub done_put_queries: Vec<(Id, Option<PutError>)>,
    /// A value response received for an in-flight GET query.
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
