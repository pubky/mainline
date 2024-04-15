//! Manage iterative queries and their corresponding request/response.

use std::collections::HashSet;
use std::net::SocketAddr;

use flume::Sender;
use tracing::{debug, error, info, trace, warn};

use super::socket::KrpcSocket;
use crate::common::{Id, Node, Response, ResponseSender, RoutingTable};
use crate::messages::{
    ErrorSpecific, PutRequest, PutRequestSpecific, RequestSpecific, RequestTypeSpecific,
};
use crate::{Error, PutResult};

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
        let candidates = RoutingTable::new().with_id(target);
        let responders = RoutingTable::new().with_id(target);

        trace!(?target, ?request, "New Query");

        Self {
            target,
            request,
            candidates,
            responders,
            inflight_requests: Vec::new(),
            visited: HashSet::new(),
            senders: Vec::new(),
            responses: Vec::new(),
        }
    }

    // === Getters ===
    /// No more inflight_requests and visited addresses were reset.
    pub fn is_done(&self) -> bool {
        self.inflight_requests.is_empty() && self.visited.is_empty()
    }

    /// Return the closest responding nodes after the query is done.
    pub fn closest(&self) -> Vec<Node> {
        self.responders.closest(&self.target)
    }

    // === Public Methods ===

    /// Add a sender to the query and send all replies we found so far to it.
    pub fn add_sender(&mut self, sender: Option<ResponseSender>) {
        if let Some(sender) = sender {
            self.senders.push(sender);
            let sender = self.senders.last().unwrap();

            for response in &self.responses {
                self.send_value(sender, response.clone())
            }
        };
    }

    /// Force start query traversal by visiting closest nodes.
    pub fn start(&mut self, socket: &mut KrpcSocket) {
        self.visit_closest(socket);
    }

    pub fn add_candidate(&mut self, node: Node) {
        // ready for a ipv6 routing table?
        self.candidates.add(node);
    }

    /// Visit explicitly given addresses, and add them to the visited set.
    pub fn visit(&mut self, socket: &mut KrpcSocket, address: SocketAddr) {
        if self.visited.contains(&address) || address.is_ipv6() {
            return;
        }

        let tid = socket.request(address, self.request.clone());
        self.inflight_requests.push(tid);
        self.visited.insert(address);
    }

    /// Remove an inflight_request and return true if it existed.
    pub fn remove_inflight_request(&mut self, tid: u16) -> bool {
        if let Some(index) = self.inflight_requests.iter().position(|&x| x == tid) {
            self.inflight_requests.remove(index);

            return true;
        };

        false
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
    pub fn tick(&mut self, socket: &mut KrpcSocket) {
        if self.is_done() {
            return;
        }

        // If there are no more inflight requests, and visited is empty, then
        // last tick we didn't add any closer nodes, so we are done traversing.
        self.visit_closest(socket);

        // First we clear timedout requests.
        // If no requests remain, then visit_closest didn't add any closer nodes,
        //  so we remove all visited addresses to set the query to "done" again.
        // If any senders are still waiting for response, send None to end the iterator,
        //  then clear them too.
        self.after_tick(socket);
    }

    // === Private Methods ===

    fn send_value(&self, sender: &ResponseSender, response: Response) {
        match (sender, response) {
            (ResponseSender::Peer(s), Response::Peer(r)) => {
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

    fn visit_closest(&mut self, socket: &mut KrpcSocket) {
        let mut to_visit = self.candidates.closest(&self.target);
        to_visit.retain(|node| !self.visited.contains(&node.address));

        for node in to_visit {
            self.visit(socket, node.address);
        }
    }

    fn after_tick(&mut self, socket: &mut KrpcSocket) {
        self.inflight_requests
            .retain(|&tid| socket.inflight_requests.contains_key(&tid));

        if self.inflight_requests.is_empty() {
            info!(target = ?self.target, visited = ?self.visited.len(), "Query Done");

            // No more closer nodes to visit, and no inflight requests to wait for
            // reset the visited set.
            //
            // Effectively this sets the query to "done" again.
            // This query will then be deleted from the rpc.queries map in the next tick.
            self.visited.clear();
        }
    }
}

#[derive(Debug)]
pub struct PutQuery {
    pub target: Id,
    pub started: bool,
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
            started: false,
            stored_at: 0,
            inflight_requests: Vec::new(),
            sender,
            request,
            error: None,
        }
    }

    pub fn is_done(&self) -> bool {
        self.started && self.inflight_requests.is_empty()
    }

    pub fn start(&mut self, socket: &mut KrpcSocket, nodes: Vec<Node>) {
        let target = self.target;
        trace!(?target, "PutQuery start");

        if self.started {
            return;
        };

        self.started = true;

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

    pub fn remove_inflight_request(&mut self, tid: u16) -> bool {
        if let Some(index) = self.inflight_requests.iter().position(|&x| x == tid) {
            self.inflight_requests.remove(index);

            return true;
        };
        false
    }

    pub fn success(&mut self) {
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

    /// remove timed out requests
    pub fn tick(&mut self, socket: &mut KrpcSocket) {
        self.inflight_requests
            .retain(|&tid| socket.inflight_requests.contains_key(&tid));

        if let Some(sender) = &self.sender {
            if self.is_done() {
                let target = self.target;

                if self.stored_at == 0 {
                    if let Some(error) = self.error.clone() {
                        error!(?target, ?error, "Put Query: failed");

                        let _ = sender.send(Err(Error::QueryError(error)));
                    }
                } else {
                    info!(target = ?self.target, stored_at = ?self.stored_at, "PutQuery Done");

                    let _ = sender.send(Ok(target));
                }
            }
        }
    }
}
