//! Manage iterative queries and their corresponding request/response.

use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4};
use std::{collections::HashSet, rc::Rc};

use tracing::{debug, error, trace, warn};

use super::{socket::KrpcSocket, ClosestNodes};
use crate::{
    common::{
        ErrorSpecific, Id, Node, PutRequest, PutRequestSpecific, RequestSpecific,
        RequestTypeSpecific, MAX_BUCKET_SIZE_K,
    },
    rpc::Response,
};

/// An iterative process of concurrently sending a request to the closest known nodes to
/// the target, updating the routing table with closer nodes discovered in the responses, and
/// repeating this process until no closer nodes (that aren't already queried) are found.
#[derive(Debug)]
pub(crate) struct IterativeQuery {
    pub request: RequestSpecific,
    closest: ClosestNodes,
    responders: ClosestNodes,
    inflight_requests: Vec<u16>,
    visited: HashSet<SocketAddr>,
    responses: Vec<Response>,
    public_address_votes: HashMap<SocketAddrV4, u16>,
}

impl IterativeQuery {
    pub fn new(target: Id, request: RequestSpecific) -> Self {
        trace!(?target, ?request, "New Query");

        Self {
            request,

            closest: ClosestNodes::new(target),
            responders: ClosestNodes::new(target),

            inflight_requests: Vec::with_capacity(200),
            visited: HashSet::with_capacity(200),

            responses: Vec::with_capacity(30),

            public_address_votes: HashMap::new(),
        }
    }

    // === Getters ===

    pub fn target(&self) -> Id {
        self.responders.target()
    }

    /// Closest nodes according to other nodes.
    pub fn closest(&self) -> &ClosestNodes {
        &self.closest
    }

    /// Return the closest responding nodes after the query is done.
    pub fn responders(&self) -> &ClosestNodes {
        &self.responders
    }

    pub fn responses(&self) -> &[Response] {
        &self.responses
    }

    pub fn best_address(&self) -> Option<SocketAddrV4> {
        let mut max = 0_u16;
        let mut best_addr = None;

        for (addr, count) in self.public_address_votes.iter() {
            if *count > max {
                max = *count;
                best_addr = Some(*addr);
            };
        }

        best_addr
    }

    // === Public Methods ===

    /// Force start query traversal by visiting closest nodes.
    pub fn start(&mut self, socket: &mut KrpcSocket) {
        self.visit_closest(socket);
    }

    /// Add a candidate node to query on next tick if it is among the closest nodes.
    pub fn add_candidate(&mut self, node: Rc<Node>) {
        // ready for a ipv6 routing table?
        self.closest.add(node);
    }

    /// Add a vote for this node's address.
    pub fn add_address_vote(&mut self, address: SocketAddr) {
        match address {
            SocketAddr::V4(address) => {
                self.public_address_votes
                    .entry(address)
                    .and_modify(|counter| *counter += 1)
                    .or_insert(1);
            }
            _ => {
                // Ipv6 is not supported
            }
        }
    }

    /// Visit explicitly given addresses, and add them to the visited set.
    /// only used from the Rpc when calling bootstrapping nodes.
    pub fn visit(&mut self, socket: &mut KrpcSocket, address: SocketAddr) {
        if address.is_ipv6() {
            return;
        }

        let tid = socket.request(address, self.request.clone());
        self.inflight_requests.push(tid);

        let tid = socket.request(
            address,
            RequestSpecific {
                requester_id: Id::random(),
                request_type: RequestTypeSpecific::Ping,
            },
        );
        self.inflight_requests.push(tid);

        self.visited.insert(address);
    }

    /// Return true if a response (by transaction_id) is expected by this query.
    pub fn inflight(&self, tid: u16) -> bool {
        self.inflight_requests.contains(&tid)
    }

    /// Add a node that responded with a token as a probable storage node.
    pub fn add_responding_node(&mut self, node: Rc<Node>) {
        self.responders.add(node)
    }

    /// Store received response.
    pub fn response(&mut self, from: SocketAddr, response: Response) {
        let target = self.target();

        debug!(?target, ?response, ?from, "Query got response");

        self.responses.push(response.to_owned());
    }

    /// Query closest nodes for this query's target and message.
    ///
    /// Returns true if it is done.
    pub fn tick(&mut self, socket: &mut KrpcSocket) -> bool {
        // Visit closest nodes
        self.visit_closest(socket);

        // If no more inflight_requests are inflight in the socket (not timed out),
        // then the query is done.
        let done = !self
            .inflight_requests
            .iter()
            .any(|&tid| socket.inflight(&tid));

        if done {
            debug!(id=?self.target(), candidates = ?self.closest.len(), visited = ?self.visited.len(), responders = ?self.responders.len(), "Done query");
        };

        done
    }

    // === Private Methods ===

    /// Visit the closest candidates and remove them as candidates
    fn visit_closest(&mut self, socket: &mut KrpcSocket) {
        let to_visit = self
            .closest
            .nodes()
            .iter()
            .take(MAX_BUCKET_SIZE_K)
            .filter(|node| !self.visited.contains(&node.address))
            .map(|node| node.address)
            .collect::<Vec<_>>();

        for address in to_visit {
            self.visit(socket, address);
        }
    }
}

#[derive(Debug)]
/// Once an [IterativeQuery] is done, or if a previous cached one was a vailable,
/// we can store data at the closest nodes using this PutQuery, that keeps track of
/// acknowledging nodes, and or errors.
pub struct PutQuery {
    pub target: Id,
    /// Nodes that confirmed success
    stored_at: u8,
    inflight_requests: Vec<u16>,
    request: PutRequestSpecific,
    error: Option<ErrorSpecific>,
}

impl PutQuery {
    pub fn new(target: Id, request: PutRequestSpecific) -> Self {
        Self {
            target,
            stored_at: 0,
            inflight_requests: Vec::new(),
            request,
            error: None,
        }
    }

    pub fn start(
        &mut self,
        socket: &mut KrpcSocket,
        nodes: Box<[Rc<Node>]>,
    ) -> Result<(), PutError> {
        // Already started.
        if !self.inflight_requests.is_empty() {
            panic!("should not call PutQuery.start() twice");
        };

        let target = self.target;
        trace!(?target, "PutQuery start");

        if nodes.is_empty() {
            Err(PutError::NoClosestNodes)?;
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

    pub fn inflight(&self, tid: u16) -> bool {
        self.inflight_requests.contains(&tid)
    }

    pub fn success(&mut self) {
        debug!(target = ?self.target, "PutQuery got success response");
        self.stored_at += 1
    }

    pub fn error(&mut self, error: ErrorSpecific) {
        if error.code >= 300 && error.code < 400 {
            warn!(target = ?self.target, ?error, "PutQuery got 3xx error");
            self.error = Some(error)
        } else {
            debug!(target = ?self.target, ?error, "PutQuery got non-3xx error");
        }
    }

    /// Check if the query is done, and if so send the query target to the receiver if any.
    pub fn tick(&mut self, socket: &mut KrpcSocket) -> Result<bool, PutError> {
        if
        // Already started
        self.inflight_requests.capacity() > 0
        // And all queries got responses or timedout
            && !self
                .inflight_requests
                .iter()
                .any(|&tid| socket.inflight(&tid))
        {
            let target = self.target;
            if self.stored_at == 0 {
                if let Some(error) = self.error.clone() {
                    error!(?target, ?error, "Put Query: failed");

                    return Err(PutError::ErrorResponse(error));
                }
            }

            debug!(?target, stored_at = ?self.stored_at, "PutQuery Done");

            return Ok(true);
        }

        Ok(false)
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

    /// Put Query faild to store at any nodes, and got at least one
    /// 3xx error response
    #[error("Query Error Response")]
    ErrorResponse(ErrorSpecific),

    /// [crate::rpc::Rpc::put] query is already inflight to the same target
    #[error("Put query is already inflight to the same target: {0}")]
    PutQueryIsInflight(Id),
}
