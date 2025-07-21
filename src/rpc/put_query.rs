use tracing::{debug, trace};

use crate::{
    common::{
        ErrorSpecific, Id, PutRequest, PutRequestSpecific, RequestSpecific, RequestTypeSpecific,
    },
    Node,
};

use super::socket::KrpcSocket;

#[derive(Debug)]
/// Once an [super::IterativeQuery] is done, or if a previous cached one was a vailable,
/// we can store data at the closest nodes using this PutQuery, that keeps track of
/// acknowledging nodes, and or errors.
pub struct PutQuery {
    pub target: Id,
    /// Nodes that confirmed success
    stored_at: u8,
    inflight_requests: Vec<u32>,
    pub request: PutRequestSpecific,
    errors: Vec<(u8, ErrorSpecific)>,
    extra_nodes: Box<[Node]>,
}

impl PutQuery {
    pub fn new(target: Id, request: PutRequestSpecific, extra_nodes: Option<Box<[Node]>>) -> Self {
        Self {
            target,
            stored_at: 0,
            inflight_requests: Vec::new(),
            request,
            errors: Vec::new(),
            extra_nodes: extra_nodes.unwrap_or(Box::new([])),
        }
    }

    pub fn start(
        &mut self,
        socket: &mut KrpcSocket,
        closest_nodes: &[Node],
    ) -> Result<(), PutError> {
        if self.started() {
            panic!("should not call PutQuery::start() twice");
        };

        let target = self.target;
        trace!(?target, "PutQuery start");

        if closest_nodes.is_empty() {
            Err(PutQueryError::NoClosestNodes)?;
        }

        if closest_nodes.len() > u8::MAX as usize {
            panic!("should not send PUT query to more than 256 nodes")
        }

        for node in closest_nodes.iter().chain(self.extra_nodes.iter()) {
            // Set correct values to the request placeholders
            if let Some(token) = node.token() {
                let tid = socket.request(
                    node.address(),
                    RequestSpecific {
                        requester_id: Id::random(),
                        request_type: RequestTypeSpecific::Put(PutRequest {
                            token,
                            put_request_type: self.request.clone(),
                        }),
                    },
                );

                self.inflight_requests.push(tid);
            }
        }

        Ok(())
    }

    pub fn started(&self) -> bool {
        !self.inflight_requests.is_empty()
    }

    pub fn inflight(&self, tid: u32) -> bool {
        self.inflight_requests.contains(&tid)
    }

    pub fn success(&mut self) {
        debug!(target = ?self.target, "PutQuery got success response");
        self.stored_at += 1
    }

    pub fn error(&mut self, error: ErrorSpecific) {
        debug!(target = ?self.target, ?error, "PutQuery got error");

        if let Some(pos) = self
            .errors
            .iter()
            .position(|(_, err)| error.code == err.code)
        {
            // Increment the count of the existing error
            self.errors[pos].0 += 1;

            // Move the updated element to maintain the order (highest count first)
            let mut i = pos;
            while i > 0 && self.errors[i].0 > self.errors[i - 1].0 {
                self.errors.swap(i, i - 1);
                i -= 1;
            }
        } else {
            // Add the new error with a count of 1
            self.errors.push((1, error));
        }
    }

    /// Check if the query is done, and if so send the query target to the receiver if any.
    pub fn tick(&mut self, socket: &KrpcSocket) -> Result<bool, PutError> {
        // Didn't start yet.
        if self.inflight_requests.is_empty() {
            return Ok(false);
        }

        // And all queries got responses or timedout
        if self.is_done(socket) {
            let target = self.target;

            if self.stored_at == 0 {
                let most_common_error = self.most_common_error();

                debug!(
                    ?target,
                    ?most_common_error,
                    nodes_count = self.inflight_requests.len(),
                    "Put Query: failed"
                );

                return Err(most_common_error
                    .map(|(_, error)| error)
                    .unwrap_or(PutQueryError::Timeout.into()));
            }

            debug!(?target, stored_at = ?self.stored_at, "PutQuery Done successfully");

            return Ok(true);
        } else if let Some(most_common_error) = self.majority_nodes_rejected_put_mutable() {
            let target = self.target;

            debug!(
                ?target,
                ?most_common_error,
                nodes_count = self.inflight_requests.len(),
                "PutQuery for MutableItem was rejected by most nodes with 3xx code."
            );

            return Err(most_common_error)?;
        }

        Ok(false)
    }

    fn is_done(&self, socket: &KrpcSocket) -> bool {
        !self
            .inflight_requests
            .iter()
            .any(|&tid| socket.inflight(tid))
    }

    fn majority_nodes_rejected_put_mutable(&self) -> Option<ConcurrencyError> {
        let half = ((self.inflight_requests.len() / 2) + 1) as u8;

        if matches!(self.request, PutRequestSpecific::PutMutable(_)) {
            return self.most_common_error().and_then(|(count, error)| {
                if count >= half {
                    if let PutError::Concurrency(err) = error {
                        Some(err)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
        };

        None
    }

    fn most_common_error(&self) -> Option<(u8, PutError)> {
        self.errors
            .first()
            .and_then(|(count, error)| match error.code {
                301 => Some((*count, PutError::from(ConcurrencyError::CasFailed))),
                302 => Some((*count, PutError::from(ConcurrencyError::NotMostRecent))),
                _ => None,
            })
    }
}

#[derive(thiserror::Error, Debug, Clone)]
/// PutQuery errors
pub enum PutError {
    /// Common PutQuery errors
    #[error(transparent)]
    Query(#[from] PutQueryError),

    #[error(transparent)]
    /// PutQuery for [crate::MutableItem] errors
    Concurrency(#[from] ConcurrencyError),
}

#[derive(thiserror::Error, Debug, Clone)]
/// Common PutQuery errors
pub enum PutQueryError {
    /// Failed to find any nodes close, usually means dht node failed to bootstrap,
    /// so the routing table is empty. Check the machine's access to UDP socket,
    /// or find better bootstrapping nodes.
    #[error("Failed to find any nodes close to store value at")]
    NoClosestNodes,

    /// Either Put Query faild to store at any nodes, and most nodes responded
    /// with a non `301` nor `302` errors.
    ///
    /// Either way; contains the most common error response.
    #[error("Query Error Response")]
    ErrorResponse(ErrorSpecific),

    /// PutQuery timed out with no responses neither success or errors
    #[error("PutQuery timed out with no responses neither success or errors")]
    Timeout,
}

#[derive(thiserror::Error, Debug, Clone)]
/// PutQuery for [crate::MutableItem] errors
pub enum ConcurrencyError {
    /// Trying to PUT mutable items with the same `key`, and `salt` but different `seq`.
    ///
    /// Moreover, the more recent item does _NOT_ mention the the earlier
    /// item's `seq` in its `cas` field.
    ///
    /// This risks a [Lost Update Problem](https://en.wikipedia.org/wiki/Write-write_conflict).
    ///
    /// Try reading most recent mutable item before writing again,
    /// and make sure to set the `cas` field.
    #[error("Conflict risk, try reading most recent item before writing again.")]
    ConflictRisk,

    /// The [crate::MutableItem::seq] is less than or equal the sequence from another signed item.
    ///
    /// Try reading most recent mutable item before writing again.
    #[error("MutableItem::seq is not the most recent, try reading most recent item before writing again.")]
    NotMostRecent,

    /// The `CAS` condition does not match the `seq` of the most recent knonw signed item.
    #[error("CAS check failed, try reading most recent item before writing again.")]
    CasFailed,
}
