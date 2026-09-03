// Roadmap milestone 1a: minimal safe IPv4 client and low-level API.

use std::{
    net::{Ipv4Addr, SocketAddrV4},
    num::NonZeroUsize,
    time::{Duration, Instant},
};

use futures_lite::Stream;

// Configuration, bootstrap, health and lifecycle used by both API levels.
// Dht is cloneable. Dht handles and active streams keep the library-owned Mio
// reactor alive; dropping the last holder stops and joins it.

impl DhtBuilder {
    pub fn network_profile(self, profile: NetworkProfile) -> Self;
    pub fn bind_address(self, address: Ipv4Addr) -> Self;
    pub fn port(self, port: u16) -> Self;
    // Overrides public-address discovery for BEP 42 ID generation.
    pub fn public_ipv4(self, address: Ipv4Addr) -> Self;
    // Replaces the selected profile's bootstrap nodes. An empty list disables
    // bootstrap seeds.
    pub fn bootstrap_nodes(self, nodes: Box<[BootstrapSeed]>) -> Self;
    // Extends the selected profile's bootstrap nodes. Testnet profiles never
    // inherit Mainline defaults.
    pub fn extra_bootstrap_nodes(self, nodes: Box<[BootstrapSeed]>) -> Self;
    pub fn query_deadlines(self, deadlines: QueryDeadlines) -> Self;
    pub async fn build(self) -> Result<Dht, DhtError>;
}

impl Dht {
    pub fn health(&self) -> DhtHealth;
    pub async fn wait_for_bootstrap(&self) -> Result<BootstrapOutcome, DhtError>;
    // Returns validated, responsive routing candidates suitable for persistent
    // bootstrap caching. Tokens and other per-operation state are excluded.
    pub async fn export_bootstrap_nodes(
        &self,
    ) -> Result<Box<[SocketAddrV4]>, DhtError>;
}

pub enum NetworkProfile {
    Mainline,
    Testnet {
        expected_nodes: NonZeroUsize,
        bootstrap_nodes: Box<[SocketAddrV4]>,
    },
}

#[non_exhaustive]
pub enum BootstrapSeed {
    Address(SocketAddrV4),
    DnsName {
        name: Box<str>,
        port: u16,
    },
}

// User-facing deadlines are policy. Per-request deadlines, pacing, and
// concurrency remain adaptive implementation details of the reactor.
pub struct QueryDeadlines {
    // Maximum time waiting for bounded reactor admission.
    pub admission: Duration,

    // Starts only after the reactor accepts the operation.
    pub execution: Duration,

    // Caps admission and execution together.
    pub overall: Duration,
}

pub struct DhtHealth {
    pub identity: Ipv4Identity,
    pub local_address: SocketAddrV4,
    pub state: DhtHealthState,
    pub routing_nodes: usize,
    pub responsive_nodes: usize,
    // Ability to send queries and receive their responses. This is not inbound
    // reachability for server mode.
    pub connectivity: Connectivity,
    pub recent_activity: NetworkActivity,
    pub bootstrap: BootstrapStatus,
}

pub struct Ipv4Identity {
    pub node_id: Id,
    pub public_address: PublicAddressStatus,
}

pub enum PublicAddressStatus {
    Unknown,
    Configured {
        address: Ipv4Addr,
    },
    Corroborated {
        address: SocketAddrV4,
        independent_observers: NonZeroUsize,
    },
}

pub enum DhtHealthState {
    Bootstrapping,
    Ready,
    Degraded,
}

pub enum Connectivity {
    Unknown,
    Reachable,
    Unreachable,
}

// Counts and their observation window are exposed instead of a lossy rate so
// callers can interpret health according to their own policy.
pub struct NetworkActivity {
    pub window: Duration,
    pub requests_sent: usize,
    pub responses_received: usize,
    pub timed_out: usize,
}

pub struct BootstrapStatus {
    // DNS resolution and DHT traversal may progress at the same time.
    pub resolution: BootstrapResolution,
    pub traversal: BootstrapTraversal,
}

pub struct BootstrapResolution {
    pub pending_names: usize,
    pub resolved_names: usize,
    pub resolved_addresses: usize,
    pub failed_names: usize,
}

pub enum BootstrapTraversal {
    NotStarted,
    InProgress(BootstrapProgress),
    Complete(BootstrapTraversalOutcome),
}

pub struct BootstrapProgress {
    pub queried: usize,
    pub responded: usize,
    pub pending: usize,
}

pub struct BootstrapOutcome {
    pub resolution: BootstrapResolution,
    pub traversal: BootstrapTraversalOutcome,
}

pub struct BootstrapTraversalOutcome {
    pub state: DhtHealthState,
    pub queried: usize,
    pub responded: usize,
    pub timed_out: usize,
}

pub enum DhtError {
    Io(std::io::Error),
    Shutdown,
}

// Low-level API: network operations and validated protocol events.

impl Dht {
    pub async fn get_mutable_responses(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
        more_recent_than: Option<i64>,
    ) -> Result<MutableLookupStream, QueryError>;

    pub async fn put_mutable_events(
        &self,
        item: MutableItem,
    ) -> Result<MutablePutStream, QueryError>;
}

pub struct MutableLookupStream {
    // Polling drains its bounded event buffer and permits this query to make
    // progress. Dropping the stream cancels the query.
}

impl Stream for MutableLookupStream {
    type Item = MutableLookupEvent;
}

impl MutableLookupStream {
    // Public policy context used by independent high-level adapters.
    pub fn network_profile(&self) -> &NetworkProfile;

    // The optional BEP 44 sequence supplied to the lookup.
    pub fn more_recent_than(&self) -> Option<i64>;
}

pub enum MutableLookupEvent {
    // Emitted for each accepted, validated response from a node.
    Response(MutableNodeResponse),

    // Emitted for a rejected response associated with this lookup. Unmatched
    // UDP packets are not exposed through the query stream.
    Rejected {
        source: SocketAddrV4,
        node_id: Option<Id>,
        reason: RejectionReason,
    },

    // Emitted when the closest set, a request state, or traversal state changes.
    Progress(MutableLookupProgress),

    // The final low-level event.
    Completed {
        // Carries the final cumulative report as part of the consistent
        // progress snapshot.
        progress: MutableLookupProgress,
        completion: MutableLookupCompletion,
    },
}

pub struct MutableNodeResponse {
    pub node: Node,
    pub received_at: Instant,
    pub rtt: Duration,

    // Candidates claimed by this node to be close to the lookup target.
    // An omitted wire field is normalized to an empty slice.
    pub candidates: Box<[Node]>,

    pub result: MutableNodeResult,
}

pub enum MutableNodeResult {
    Item(MutableItem),
    NoValue,
    NoMoreRecent { sequence: i64 },
}

#[non_exhaustive]
pub enum RejectionReason {
    Malformed,
    ProtocolViolation,
    UnexpectedSource,
    NonCompliantNodeId,
    InvalidMutableItem,
}

pub struct MutableLookupProgress {
    // Private fields preserve one consistent traversal snapshot.
}

impl MutableLookupProgress {
    pub fn closest_set(&self) -> &ClosestSetSnapshot;
    pub fn traversal_converged(&self) -> bool;

    // A cumulative snapshot lets an independent adapter produce a partial
    // report when it completes early and drops the low-level stream.
    pub fn report(&self) -> &MutableGetReport;
}

pub struct ClosestSetSnapshot {
    // Private fields preserve membership, ordering, and status consistency.
}

impl ClosestSetSnapshot {
    // Changes only when membership or ordering changes, not node status.
    pub fn changed_at(&self) -> Instant;

    // Contains the latest status for every member of the current closest set.
    pub fn members(&self) -> &[ClosestNodeProgress];
}

pub struct ClosestNodeProgress {
    // Private fields preserve the relationship between a node and its state.
}

impl ClosestNodeProgress {
    pub fn node(&self) -> &Node;
    pub fn state(&self) -> &ClosestNodeState;
}

pub enum ClosestNodeState {
    NotContacted,
    InFlight,
    Responded,
    Failed,
}

pub enum MutableLookupCompletion {
    TraversalComplete,
    QueryTimeout,
    OverallDeadline,
    Unreachable,
    Shutdown,
}

pub struct MutablePutStream {
    // Polling drains its bounded event buffer and permits this query to make
    // progress. Dropping the stream cancels the query.
}

impl Stream for MutablePutStream {
    type Item = MutablePutEvent;
}

impl MutablePutStream {
    // Public policy context used by independent high-level adapters.
    pub fn network_profile(&self) -> &NetworkProfile;

    // The requested item is operation metadata, not private reactor state.
    pub fn item(&self) -> &MutableItem;
}

pub enum MutablePutEvent {
    // Progress while locating the closest nodes and obtaining write tokens.
    LookupProgress(MutableLookupProgress),

    Acknowledged {
        node: Node,
    },

    // A raw 301 or 302 response; it is not itself evidence of a conflict.
    RejectionClaim {
        node: Node,
        code: MutablePutErrorCode,
    },

    // Result of challenging a rejection claim with a mutable GET.
    Verification {
        node: Node,
        result: MutableNodeResult,
    },

    // The final low-level event.
    Completed {
        completion: MutablePutCompletion,
        report: MutablePutReport,
    },
}

pub enum MutablePutErrorCode {
    CasMismatch,
    SequenceTooLow,
}

pub enum MutablePutCompletion {
    PublicationComplete,
    QueryTimeout,
    OverallDeadline,
    Unreachable,
    Shutdown,
}

pub struct MutablePutReport {
    pub requests_sent: usize,
    pub responses_received: usize,
    pub attempted_targets: usize,
    pub target_set_size: usize,
    pub acknowledgements: usize,
    pub rejection_claims: usize,
    pub verified_conflicts: usize,
    pub protocol_errors: usize,
    pub timed_out: usize,
}

pub struct MutableGetReport {
    pub requests_sent: usize,
    pub responses_received: usize,
    pub unique_responders: usize,
    // Cumulative validated, unique response categories across the lookup.
    // These are diagnostics, not current closest-set coverage.
    pub item_responses: usize,
    pub no_value_responses: usize,
    pub no_more_recent_responses: usize,
    pub rejected_responses: usize,
    pub protocol_errors: usize,
    pub final_closest_nodes: usize,
    pub timed_out: usize,
    pub ignored_stragglers: usize,
}

pub enum QueryError {
    AdmissionTimeout,
    OverallDeadline,
    Shutdown,
}
