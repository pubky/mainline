// Simplified high-level mutable API sketch.
//
// This file is for design discussion. The implementation is an adapter over
// the public low-level streams and intentionally omits supporting details.

impl Dht {
    pub async fn get_mutable(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
        more_recent_than: Option<i64>,
    ) -> Result<MutableEstimateStream, QueryError>;

    pub async fn put_mutable(
        &self,
        item: MutableItem,
    ) -> Result<MutablePutConclusion, QueryError>;
}

pub struct MutableEstimateStream;

impl Stream for MutableEstimateStream {
    type Item = MutableEstimateUpdate;
}

pub struct MutableEstimateUpdate {
    pub status: MutableGetStatus,
    pub evidence: MutableGetEvidence,
}

pub enum MutableGetStatus {
    Searching(Option<MutableItem>),
    Converged(MutableItem),
    NotFound,
    NoNewerItem,
    Inconclusive {
        estimate: Option<MutableItem>,
        reason: MutableGetInconclusiveReason,
    },
}

pub struct MutableGetEvidence {
    pub covered_nodes: usize,
    pub closest_nodes: usize,
    pub pending_nodes: usize,
    pub traversal_converged: bool,
}

pub enum MutableGetInconclusiveReason {
    Timeout,
    Unreachable,
    Shutdown,
    ProtocolError,
    InsufficientCoverage,
}

pub enum MutablePutConclusion {
    Published {
        acknowledgements: NonZeroUsize,
        evidence: MutablePutEvidence,
    },
    Conflict {
        newer: MutableItem,
        evidence: MutablePutEvidence,
    },
    Inconclusive {
        reason: MutablePutInconclusiveReason,
        evidence: MutablePutEvidence,
    },
}

pub struct MutablePutEvidence {
    pub attempted_nodes: usize,
    pub acknowledgements: usize,
    pub timed_out: usize,
}

pub enum MutablePutInconclusiveReason {
    NoAcknowledgement,
    Timeout,
    Unreachable,
    Shutdown,
    ProtocolError,
}
