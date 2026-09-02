use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddrV4},
    num::NonZeroUsize,
    time::{Duration, Instant},
};

use futures_lite::Stream;

// Configuration, bootstrap, health and lifecycle shared by both API levels.
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

pub enum BootstrapStatus {
    Resolving(BootstrapResolutionProgress),
    InProgress(BootstrapProgress),
    Complete(BootstrapOutcome),
}

pub struct BootstrapResolutionProgress {
    pub pending_names: usize,
    pub resolved_addresses: usize,
    pub failed_names: usize,
}

pub struct BootstrapProgress {
    pub queried: usize,
    pub responded: usize,
    pub pending: usize,
}

pub struct BootstrapOutcome {
    pub state: DhtHealthState,
    pub dns_failures: usize,
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

// High-level API: adapters over only the public low-level streams above. This
// logic requires no access to the reactor, routing table, or RPC internals.

impl Dht {
    pub async fn get_mutable(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
        more_recent_than: Option<i64>,
    ) -> Result<MutableEstimateStream, QueryError> {
        let responses = self
            .get_mutable_responses(public_key, salt, more_recent_than)
            .await?;
        Ok(responses.into_estimates())
    }

    pub async fn get_mutable_with_policy(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
        more_recent_than: Option<i64>,
        policy: MutableGetPolicy,
    ) -> Result<MutableEstimateStream, QueryError> {
        let responses = self
            .get_mutable_responses(public_key, salt, more_recent_than)
            .await?;
        Ok(responses.into_estimates_with_policy(policy))
    }

    pub async fn put_mutable(
        &self,
        item: MutableItem,
    ) -> Result<MutablePutConclusion, QueryError> {
        let events = self.put_mutable_events(item).await?;
        Ok(events.into_conclusion().await)
    }
}

impl MutableLookupStream {
    // Uses MutableGetPolicy::Adaptive.
    pub fn into_estimates(self) -> MutableEstimateStream;
    pub fn into_estimates_with_policy(self, policy: MutableGetPolicy)
        -> MutableEstimateStream;
}

impl MutablePutStream {
    pub fn into_conclusion(self) -> MutablePutConclusionFuture;
}

pub struct MutablePutConclusionFuture {
    // Private fields contain only a MutablePutStream and adapter state.
}

impl Future for MutablePutConclusionFuture {
    type Output = MutablePutConclusion;
}

pub enum MutablePutConclusion {
    Published(MutablePutPublished),
    Conflict(MutablePutConflict),
    Inconclusive(MutablePutInconclusive),
}

// Private fields keep conclusion-specific invariants intact. In particular, a
// Published conclusion always has at least one acknowledgement.
pub struct MutablePutPublished {
    // Private fields.
}

impl MutablePutPublished {
    pub fn acknowledgements(&self) -> NonZeroUsize;
    pub fn evidence(&self) -> &MutablePutEvidence;
}

pub struct MutablePutConflict {
    // Private fields.
}

impl MutablePutConflict {
    pub fn newer(&self) -> &MutableItem;
    pub fn evidence(&self) -> &MutablePutEvidence;
}

pub struct MutablePutInconclusive {
    // Private fields.
}

impl MutablePutInconclusive {
    pub fn reason(&self) -> &MutablePutInconclusiveReason;
    pub fn evidence(&self) -> &MutablePutEvidence;
}

pub enum MutablePutInconclusiveReason {
    NoAcknowledgement,
    QueryTimeout,
    OverallDeadline,
    Unreachable,
    Shutdown,
    ProtocolError,
}

pub struct MutablePutEvidence {
    // Private fields derived solely from MutablePutEvent values.
}

impl MutablePutEvidence {
    pub fn report(&self) -> &MutablePutReport;
    pub fn acknowledgements(&self) -> usize;
    pub fn attempted_targets(&self) -> usize;
    pub fn target_set_size(&self) -> usize;
}

pub struct MutableEstimateStream {
    // Private fields contain only a MutableLookupStream and adapter state.
    // It therefore has the same stream-driven execution and cancellation.
}

impl Stream for MutableEstimateStream {
    type Item = MutableEstimateUpdate;
}

pub enum MutableGetPolicy {
    // Return after traversal, sufficient relative coverage, and adaptive
    // settling. Slow stragglers need not hold the query open.
    Adaptive,

    // Wait for every relevant request to receive a response or time out.
    Strict,
}

pub struct MutableEstimateUpdate {
    pub evidence: MutableGetEvidence,
    pub status: MutableGetStatus,
}

pub enum MutableGetStatus {
    Searching {
        estimate: Option<MutableEstimate>,
    },
    Converged {
        estimate: MutableEstimate,
        report: MutableGetReport,
    },
    NotFound {
        report: MutableGetReport,
    },
    Inconclusive {
        estimate: Option<MutableEstimate>,
        reason: MutableGetInconclusiveReason,
        report: MutableGetReport,
    },
}

pub struct MutableEstimate {
    // Private fields preserve consistency with the accompanying evidence.
}

impl MutableEstimate {
    pub fn item(&self) -> &MutableItem;
    pub fn supporting_nodes(&self) -> usize;
}

pub struct MutableGetEvidence {
    // Private fields derived from one MutableLookupProgress snapshot.
}

impl MutableGetEvidence {
    // Coverage values refer only to unique valid responses from the current
    // closest set. Together they fully explain the adapter's decision.
    pub fn covered_nodes(&self) -> usize;
    pub fn coverage_basis(&self) -> usize;
    pub fn required_coverage(&self) -> usize;
    pub fn allowed_outstanding(&self) -> usize;

    pub fn closest_nodes(&self) -> usize;
    pub fn pending(&self) -> usize;
    pub fn failed(&self) -> usize;
    pub fn unchanged_for(&self) -> Duration;
    pub fn settling_period(&self) -> Duration;
    pub fn traversal_converged(&self) -> bool;
}

pub enum MutableGetInconclusiveReason {
    QueryTimeout,
    OverallDeadline,
    Unreachable,
    Shutdown,
    ProtocolError,
    InsufficientCoverage,
}
