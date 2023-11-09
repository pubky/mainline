use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};

use crate::common::{Id, Node};
use crate::messages::{Message, RequestSpecific};
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
    // TODO add last refresed
}

impl Query {
    pub fn new(target: Id, request: RequestSpecific) -> Self {
        let mut table = RoutingTable::new().with_id(target);

        Self {
            target,
            request,
            table,
            inflight_requests: Vec::new(),
            visited: HashSet::new(),
        }
    }

    // === Getters ===
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub fn is_done(&self) -> bool {
        self.inflight_requests.is_empty()
    }

    pub fn closest(&self, target: &Id) -> Vec<Node> {
        self.table.closest(&self.target)
    }

    // === Public Methods ===

    /// Add a node to the correct routing table.
    pub fn add(&mut self, node: Node) {
        // ready for a ipv6 routing table?
        self.table.add(node);
    }

    pub fn visit(&mut self, socket: &mut KrpcSocket, address: SocketAddr) {
        if self.visited.contains(&address) || address.is_ipv6() {
            // TODO: Add support for IPV6.
            return;
        }

        let tid = socket.request(address, self.request.clone());
        self.inflight_requests.push(tid);
        self.visited.insert(address);
    }

    /// If the closer nodes are from a response to a request sent by this query, return true.
    pub fn add_closer_nodes(&mut self, tid: u16, from: SocketAddr, nodes: Vec<Node>) -> bool {
        if let Some(index) = self.inflight_requests.iter().position(|&x| x == tid) {
            self.inflight_requests.remove(index);

            for node in nodes {
                self.table.add(node);
            }

            return true;
        };

        false
    }

    /// Query closest nodes for this query's target and message.
    pub fn tick(&mut self, socket: &mut KrpcSocket) {
        self.timeout(socket);
        self.next(socket);
    }

    // === Private Methods ===
    fn next(&mut self, socket: &mut KrpcSocket) {
        let mut to_visit = self.table.closest(&self.target);
        to_visit.retain(|node| !self.visited.contains(&node.address));

        if to_visit.is_empty() && self.inflight_requests.is_empty() {
            // No more closer nodes to visit, and no inflight requests to wait for
            // reset the visited set.
            self.finish();

            return;
        }

        for node in to_visit {
            self.visit(socket, node.address);
        }
    }

    /// Remove timed out requests.
    fn timeout(&mut self, socket: &KrpcSocket) {
        self.inflight_requests
            .retain(|&tid| socket.inflight_requests.contains_key(&tid));
    }

    fn finish(&mut self) {
        self.visited.clear();
    }
}
