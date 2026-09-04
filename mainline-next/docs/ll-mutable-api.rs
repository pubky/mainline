// Simplified low-level mutable API sketch.
//
// This file is for design discussion. It intentionally omits supporting traits,
// metadata, detailed counters, and some error variants.

impl Dht {
    pub async fn get_mutable_events(
        &self,
        public_key: &[u8; 32],
        salt: Option<&[u8]>,
        more_recent_than: Option<i64>,
    ) -> Result<MutableGetStream, QueryError>;

    pub async fn put_mutable_events(
        &self,
        item: MutableItem,
    ) -> Result<MutablePutStream, QueryError>;
}

pub struct MutableGetStream;

impl Stream for MutableGetStream {
    type Item = MutableGetEvent;
}

pub enum MutableGetEvent {
    Response {
        node: Node,
        rtt: Duration,
        result: MutableNodeResult,
    },
    Rejected {
        source: SocketAddrV4,
        reason: RejectionReason,
    },
    Progress(MutableGetProgress),
    Finished {
        completion: QueryCompletion,
        report: MutableGetReport,
    },
}

pub enum MutableNodeResult {
    Item(MutableItem),
    NoValue,
    NoMoreRecent { sequence: i64 },
}

pub struct MutableGetProgress {
    pub closest_nodes: Box<[NodeProgress]>,
    pub traversal_converged: bool,
}

pub struct NodeProgress {
    pub node: Node,
    pub state: NodeState,
}

pub enum NodeState {
    NotContacted,
    InFlight,
    Responded,
    Failed,
}

pub struct MutablePutStream;

impl Stream for MutablePutStream {
    type Item = MutablePutEvent;
}

pub enum MutablePutEvent {
    LookupProgress(MutableGetProgress),
    Acknowledged(Node),
    RejectionClaim {
        node: Node,
        code: MutablePutErrorCode,
    },
    Verification {
        node: Node,
        result: MutableNodeResult,
    },
    Finished {
        completion: QueryCompletion,
        report: MutablePutReport,
    },
}

pub enum QueryCompletion {
    Complete,
    Timeout,
    Unreachable,
    Shutdown,
}

pub enum MutablePutErrorCode {
    CasMismatch,
    SequenceTooLow,
}

pub struct MutableGetReport {
    pub responses: usize,
    pub rejected: usize,
    pub timed_out: usize,
}

pub struct MutablePutReport {
    pub attempted_nodes: usize,
    pub acknowledgements: usize,
    pub verified_conflicts: usize,
    pub timed_out: usize,
}

pub enum RejectionReason {
    Malformed,
    UnexpectedSource,
    NonCompliantNodeId,
    InvalidItem,
}
