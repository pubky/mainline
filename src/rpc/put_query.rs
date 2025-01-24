use std::rc::Rc;

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
    inflight_requests: Vec<u16>,
    pub request: PutRequestSpecific,
    errors: Vec<(u8, ErrorSpecific)>,
}

impl PutQuery {
    pub fn new(target: Id, request: PutRequestSpecific) -> Self {
        Self {
            target,
            stored_at: 0,
            inflight_requests: Vec::new(),
            request,
            errors: Vec::new(),
        }
    }

    pub fn start(
        &mut self,
        socket: &mut KrpcSocket,
        nodes: Box<[Rc<Node>]>,
    ) -> Result<(), PutError> {
        if self.started() {
            panic!("should not call PutQuery::start() twice");
        };

        let target = self.target;
        trace!(?target, "PutQuery start");

        if nodes.is_empty() {
            Err(PutError::NoClosestNodes)?;
        }

        if nodes.len() > u8::MAX as usize {
            panic!("should not send PUT query to more than 256 nodes")
        }

        for node in nodes {
            // Set correct values to the request placeholders
            if let Some(token) = node.token.clone() {
                let tid = socket.request(
                    node.address,
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

    pub fn inflight(&self, tid: u16) -> bool {
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
                let most_common_error = self.errors.first();

                debug!(
                    ?target,
                    ?most_common_error,
                    nodes_count = self.inflight_requests.len(),
                    "Put Query: failed"
                );

                return Err(most_common_error
                    .map(|(_, error)| PutError::ErrorResponse(error.clone()))
                    .unwrap_or(PutError::Timeout));
            }

            debug!(?target, stored_at = ?self.stored_at, "PutQuery Done successfully");

            return Ok(true);
        } else if self.majority_nodes_rejected_put_mutable() {
            let target = self.target;
            let most_common_error = self.most_common_error();

            debug!(
                ?target,
                ?most_common_error,
                nodes_count = self.inflight_requests.len(),
                "PutQuery for MutableItem was rejected by most nodes with 3xx code."
            );

            return Err(most_common_error
                .map(|(_, error)| PutError::ErrorResponse(error.clone()))
                .unwrap_or(PutError::Timeout));
        }

        Ok(false)
    }

    fn is_done(&self, socket: &KrpcSocket) -> bool {
        !self
            .inflight_requests
            .iter()
            .any(|&tid| socket.inflight(&tid))
    }

    fn majority_nodes_rejected_put_mutable(&self) -> bool {
        let half = ((self.inflight_requests.len() / 2) + 1) as u8;

        matches!(self.request, PutRequestSpecific::PutMutable(_))
            && self
                .most_common_error()
                .map(|(count, error)| (error.code == 301 || error.code == 302) && *count >= half)
                .unwrap_or(false)
    }

    fn most_common_error(&self) -> Option<&(u8, ErrorSpecific)> {
        self.errors.first()
    }
}

#[derive(thiserror::Error, Debug, Clone)]
/// Query errors
pub enum PutError {
    /// Failed to find any nodes close, usually means dht node failed to bootstrap,
    /// so the routing table is empty. Check the machine's access to UDP socket,
    /// or find better bootstrapping nodes.
    #[error("Failed to find any nodes close to store value at")]
    NoClosestNodes,

    /// Either Put Query faild to store at any nodes,
    /// OR during [PUT_MUTABLE](https://www.bittorrent.org/beps/bep_0044.html) request, majority of nodes responded with a 3xx error.
    ///
    /// Either way; contains the most common error response.
    #[error("Query Error Response")]
    ErrorResponse(ErrorSpecific),

    /// PutQuery timed out with no responses neither success or errors
    #[error("PutQuery timed out with no responses neither success or errors")]
    Timeout,

    /// Calling [crate::rpc::Rpc::put] twice for the same target with different
    /// [crate::MutableItem] risks losing data.
    #[error("Concurrent PUT queries for different mutable items with the same target ({0})")]
    ConcurrentPutMutable(Id),
}
