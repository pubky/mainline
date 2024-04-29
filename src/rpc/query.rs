//! Manage iterative queries and their corresponding request/response.

use std::collections::HashSet;
use std::net::SocketAddr;

use flume::Sender;
use tracing::{debug, error, info, trace, warn};

use super::socket::KrpcSocket;
use crate::Error;
use crate::{
    common::{
        ErrorSpecific, Id, Node, PutRequest, PutRequestSpecific, RequestSpecific,
        RequestTypeSpecific, RoutingTable,
    },
    rpc::{PutResult, Response, ResponseSender},
};

/// A query is an iterative process of concurrently sending a request to the closest known nodes to
/// the target, updating the routing table with closer nodes discovered in the responses, and
/// repeating this process until no closer nodes (that aren't already queried) are found.
#[derive(Debug)]
pub struct Query {
    pub target: Id,
    pub request: RequestSpecific,
    candidates: RoutingTable,
    responders: RoutingTable,
    inflight_requests: Vec<u16>,
    visited: HashSet<SocketAddr>,
    senders: Vec<ResponseSender>,
    responses: Vec<Response>,
}

impl Query {
    pub fn new(target: Id, request: RequestSpecific) -> Self {
        trace!(?target, ?request, "New Query");

        Self {
            target,
            request,

            candidates: RoutingTable::new().with_id(target),
            responders: RoutingTable::new().with_id(target),

            inflight_requests: Vec::with_capacity(200),
            visited: HashSet::with_capacity(200),

            senders: Vec::with_capacity(1),
            responses: Vec::with_capacity(30),
        }
    }

    // === Getters ===

    /// Return the closest responding nodes after the query is done.
    pub fn closest(&self) -> Vec<Node> {
        self.responders.closest(&self.target)
    }

    // === Public Methods ===

    /// Add a sender to the query and send all replies we found so far to it.
    pub fn add_sender(&mut self, sender: ResponseSender) {
        for response in &self.responses {
            self.send_value(&sender, response.clone())
        }

        self.senders.push(sender);
    }

    /// Force start query traversal by visiting closest nodes.
    pub fn start(&mut self, socket: &mut KrpcSocket) {
        self.visit_closest(socket);
    }

    /// Add a candidate node to query on next tick if it is among the closest nodes.
    pub fn add_candidate(&mut self, node: Node) {
        // ready for a ipv6 routing table?
        self.candidates.add(node);
    }

    /// Visit explicitly given addresses, and add them to the visited set.
    /// only used from the Rpc when calling bootstrapping nodes.
    pub fn visit(&mut self, socket: &mut KrpcSocket, address: SocketAddr) {
        if address.is_ipv6() {
            return;
        }

        let tid = socket.request(address, self.request.clone());
        self.inflight_requests.push(tid);
        self.visited.insert(address);
    }

    /// Return true if a response (by transaction_id) is expected by this query.
    pub fn inflight(&self, tid: u16) -> bool {
        self.inflight_requests.contains(&tid)
    }

    /// Add a node that responded with a token as a probable storage node.
    pub fn add_responding_node(&mut self, node: Node) {
        self.responders.add(node.clone());
    }

    /// Add received response
    pub fn response(&mut self, from: SocketAddr, response: Response) {
        let target = self.target;

        debug!(?target, ?response, ?from, "Query got response");

        for sender in &self.senders {
            self.send_value(sender, response.to_owned())
        }

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
        !self
            .inflight_requests
            .iter()
            .any(|&tid| socket.inflight(&tid))
    }

    // === Private Methods ===

    fn send_value(&self, sender: &ResponseSender, response: Response) {
        match (sender, response) {
            (ResponseSender::Peers(s), Response::Peers(r)) => {
                let _ = s.send(r);
            }
            (ResponseSender::Mutable(s), Response::Mutable(r)) => {
                let _ = s.send(r);
            }
            (ResponseSender::Immutable(s), Response::Immutable(r)) => {
                let _ = s.send(r);
            }
            _ => {}
        }
    }

    /// Visit the closest candidates and remove them as candidates
    fn visit_closest(&mut self, socket: &mut KrpcSocket) {
        for node in self.candidates.closest(&self.target) {
            if !self.visited.contains(&node.address) {
                self.visit(socket, node.address);
            }
        }
    }
}

#[derive(Debug)]
pub struct PutQuery {
    pub target: Id,
    /// Nodes that confirmed success
    stored_at: u8,
    inflight_requests: Vec<u16>,
    sender: Option<Sender<PutResult>>,
    request: PutRequestSpecific,
    error: Option<ErrorSpecific>,
}

impl PutQuery {
    pub fn new(target: Id, request: PutRequestSpecific, sender: Option<Sender<PutResult>>) -> Self {
        Self {
            target,
            stored_at: 0,
            inflight_requests: Vec::new(),
            sender,
            request,
            error: None,
        }
    }

    pub fn start(&mut self, socket: &mut KrpcSocket, nodes: Vec<Node>) {
        // Already started.
        if !self.inflight_requests.is_empty() {
            panic!("should not call PutQuery.start() twice");
        };

        let target = self.target;
        trace!(?target, "PutQuery start");

        if let Some(sender) = &self.sender {
            if nodes.is_empty() {
                let _ = sender.send(Err(Error::NoClosestNodes));
            }
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
    pub fn tick(&mut self, socket: &mut KrpcSocket) -> bool {
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

                    let _ = self
                        .sender
                        .to_owned()
                        .map(|sender| sender.send(Err(Error::QueryError(error))));
                }
            } else {
                info!(?target, stored_at = ?self.stored_at, "PutQuery Done");

                let _ = self.sender.to_owned().map(|sender| sender.send(Ok(target)));
            }

            return true;
        }

        false
    }
}
