// Roadmap milestone 1b: high-level IPv4 item API.
//
// The types and operations below depend only on the public milestone 1a API.

use std::{future::Future, num::NonZeroUsize, time::Duration};

use futures_lite::Stream;

// High-level API: adapters over only the public low-level milestone 1a streams.
// This logic requires no access to the reactor, routing table, or RPC internals.

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

    pub async fn get_immutable(
        &self,
        target: Id,
    ) -> Result<ImmutableGetConclusion, QueryError> {
        let responses = self.get_immutable_responses(target).await?;
        Ok(responses.into_conclusion().await)
    }

    pub async fn put_immutable(
        &self,
        value: &[u8],
    ) -> Result<ImmutablePutConclusion, QueryError> {
        let events = self.put_immutable_events(value).await?;
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
    // A conditional lookup reached sufficient coverage, found no newer item,
    // and at least one node reported an existing sequence through
    // MutableNodeResult::NoMoreRecent.
    NoNewerItem {
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

impl ImmutableLookupStream {
    // Returns as soon as a hash-valid value is observed. In its absence, the
    // adapter requires converged traversal and sufficient closest-set coverage;
    // low-level completion without both produces an inconclusive result.
    pub fn into_conclusion(self) -> ImmutableGetConclusionFuture;
}

pub struct ImmutableGetConclusionFuture {
    // Private fields contain only an ImmutableLookupStream and adapter state.
}

impl Future for ImmutableGetConclusionFuture {
    type Output = ImmutableGetConclusion;
}

pub enum ImmutableGetConclusion {
    Found {
        value: Box<[u8]>,
        report: ImmutableGetReport,
    },
    NotFound {
        report: ImmutableGetReport,
    },
    Inconclusive {
        reason: ImmutableGetInconclusiveReason,
        report: ImmutableGetReport,
    },
}

pub enum ImmutableGetInconclusiveReason {
    QueryTimeout,
    OverallDeadline,
    Unreachable,
    Shutdown,
    ProtocolError,
    InsufficientCoverage,
}

impl ImmutablePutStream {
    pub fn into_conclusion(self) -> ImmutablePutConclusionFuture;
}

pub struct ImmutablePutConclusionFuture {
    // Private fields contain only an ImmutablePutStream and adapter state.
}

impl Future for ImmutablePutConclusionFuture {
    type Output = ImmutablePutConclusion;
}

pub enum ImmutablePutConclusion {
    Published {
        target: Id,
        acknowledgements: NonZeroUsize,
        report: ImmutablePutReport,
    },
    Inconclusive {
        target: Id,
        reason: ImmutablePutInconclusiveReason,
        report: ImmutablePutReport,
    },
}

pub enum ImmutablePutInconclusiveReason {
    NoAcknowledgement,
    QueryTimeout,
    OverallDeadline,
    Unreachable,
    Shutdown,
    ProtocolError,
}
