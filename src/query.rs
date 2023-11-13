use std::collections::HashSet;
use std::net::SocketAddr;

use crate::common::{Id, Node};
use crate::dht::{GetPeerResponse, ResponseItem, ResponseSender};
use crate::messages::RequestSpecific;
use crate::routing_table::RoutingTable;
use crate::socket::KrpcSocket;

/// A query is an iterative process of concurrently sending a request to the closest known nodes to
/// the target, updating the routing table with closer nodes discovered in the responses, and
/// repeating this process until no closer nodes (that aren't already queried) are found.
#[derive(Debug)]
pub struct Query {
    target: Id,
    request: RequestSpecific,
    table: RoutingTable,
    inflight_requests: Vec<u16>,
    visited: HashSet<SocketAddr>,
    senders: Vec<ResponseSender>,
    responses: Vec<ResponseItem>,
}

impl Query {
    pub fn new(target: Id, request: RequestSpecific) -> Self {
        let table = RoutingTable::new().with_id(target);

        Self {
            target,
            request,
            table,
            inflight_requests: Vec::new(),
            visited: HashSet::new(),
            senders: Vec::new(),
            responses: Vec::new(),
        }
    }

    // === Getters ===
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// No more inflight_requests and visited addresses were reset.
    pub fn is_done(&self) -> bool {
        self.inflight_requests.is_empty() && self.visited.is_empty()
    }

    pub fn closest(&self, target: &Id) -> Vec<Node> {
        self.table.closest(&self.target)
    }

    // === Public Methods ===

    /// Add a sender to the query and send all replies we found so far to it.
    pub fn add_sender(&mut self, sender: Option<ResponseSender>) {
        if let Some(sender) = sender {
            self.senders.push(sender);
            let sender = self.senders.last().unwrap();

            for response in &self.responses {
                self.send_response(sender, Some(response))
            }
        };
    }

    /// Force start query traversal by visiting closest nodes.
    pub fn start(&mut self, socket: &mut KrpcSocket) {
        self.visit_closest(socket);
    }

    /// Add a node to the routing table.
    pub fn add_node(&mut self, node: Node) {
        // ready for a ipv6 routing table?
        self.table.add(node);
    }

    /// Visit explicitly given addresses, and add them to the visited set.
    pub fn visit(&mut self, socket: &mut KrpcSocket, address: SocketAddr) {
        if self.visited.contains(&address) || address.is_ipv6() {
            // TODO: Add support for IPV6.
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

    /// Add reveived response
    pub fn response(&mut self, response: ResponseItem) {
        self.responses.push(response.clone());

        for sender in &self.senders {
            self.send_response(sender, Some(&response))
        }
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

    fn send_response(&self, sender: &ResponseSender, response: Option<&ResponseItem>) {
        match sender {
            ResponseSender::Peer(sender) => {
                if let Some(response) = response {
                    let ResponseItem::Peer(response) = response.clone();
                    let _ = sender.send(Some(response));
                } else {
                    let _ = sender.send(None);
                }
            }
            _ => {}
        };
    }

    fn visit_closest(&mut self, socket: &mut KrpcSocket) {
        let mut to_visit = self.table.closest(&self.target);
        to_visit.retain(|node| !self.visited.contains(&node.address));

        for node in to_visit {
            self.visit(socket, node.address);
        }
    }

    fn after_tick(&mut self, socket: &mut KrpcSocket) {
        self.inflight_requests
            .retain(|&tid| socket.inflight_requests.contains_key(&tid));

        if self.inflight_requests.is_empty() {
            // Send None to all receivers to end iterators
            for sender in &self.senders {
                self.send_response(sender, None);
                // println!("Visited {} nodes", self.visited.len());
            }

            // No more closer nodes to visit, and no inflight requests to wait for
            // reset the visited set.
            //
            // Effectively this sets the query to "done" again.
            // This query will then be deleted from the rpc.queries map in the next tick.
            self.visited.clear();
        }
    }
}
