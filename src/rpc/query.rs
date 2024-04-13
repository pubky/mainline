//! Manage iterative queries and their corresponding request/response.

use std::collections::HashSet;
use std::net::SocketAddr;

use flume::Sender;
use tracing::info;

use super::response::{Response, ResponseSender, StoreQueryMetdata};
use super::socket::KrpcSocket;
use crate::common::{Id, Node, RoutingTable};
use crate::messages::{PutRequest, PutRequestSpecific, RequestSpecific, RequestTypeSpecific};

/// A query is an iterative process of concurrently sending a request to the closest known nodes to
/// the target, updating the routing table with closer nodes discovered in the responses, and
/// repeating this process until no closer nodes (that aren't already queried) are found.
#[derive(Debug)]
pub struct Query {
    target: Id,
    request: RequestSpecific,
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

    pub fn target(&self) -> &Id {
        &self.target
    }

    /// Return the query's request sent to every visited node.
    pub fn request(&self) -> &RequestSpecific {
        &self.request
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

    /// Add a node to the routing table.
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
        let query = self.target;
        info!(?query, ?from, "Got value");

        for sender in &self.senders {
            self.send_value(sender, response.to_owned())
        }

        self.responses.push(response);
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
            let visited = self.visited.len();
            info!(?self.target, ?visited, "Query done");

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
pub struct StoreQuery {
    target: Id,
    /// Nodes queried
    closest_nodes: Vec<Node>,
    /// Nodes that confirmed success
    stored_at: Vec<Id>,
    inflight_requests: Vec<u16>,
    sender: Option<Sender<StoreQueryMetdata>>,
    request: PutRequestSpecific,
}

// TODO: can we make both queries the same thing?
impl StoreQuery {
    pub fn new(
        target: Id,
        request: PutRequestSpecific,
        sender: Option<Sender<StoreQueryMetdata>>,
    ) -> Self {
        Self {
            target,
            closest_nodes: Vec::new(),
            stored_at: Vec::new(),
            inflight_requests: Vec::new(),
            sender,
            request,
        }
    }

    pub fn alredy_started(&self) -> bool {
        !self.closest_nodes.is_empty()
    }

    pub fn start(&mut self, socket: &mut KrpcSocket, nodes: Vec<Node>) {
        if self.alredy_started() {
            panic!("should not start twice");
        };

        for node in nodes {
            // Set correct values to the request placeholders

            self.request(node, socket)
        }
    }

    pub fn request(&mut self, node: Node, socket: &mut KrpcSocket) {
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

            self.closest_nodes.push(node);
            self.inflight_requests.push(tid);
        }
    }

    // the closest nodes were set and all inflight_requests were done.
    pub fn is_done(&self) -> bool {
        self.inflight_requests.is_empty() && !self.closest_nodes.is_empty()
    }

    pub fn remove_inflight_request(&mut self, tid: u16) -> bool {
        if let Some(index) = self.inflight_requests.iter().position(|&x| x == tid) {
            self.inflight_requests.remove(index);

            return true;
        };
        false
    }

    pub fn success(&mut self, id: Id) {
        self.stored_at.push(id);
    }

    /// remove timed out requests
    pub fn tick(&mut self, socket: &mut KrpcSocket) {
        self.inflight_requests
            .retain(|&tid| socket.inflight_requests.contains_key(&tid));

        if self.is_done() {
            if let Some(sender) = &self.sender {
                let _ = sender.send(StoreQueryMetdata::new(
                    self.target,
                    self.closest_nodes.clone(),
                    self.stored_at.clone(),
                ));
            }
        }
    }
}
