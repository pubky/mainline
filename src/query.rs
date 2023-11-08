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
    pub request: RequestSpecific,
    pub table: RoutingTable,
    inflight_requests: Vec<u16>,
    visited: HashSet<SocketAddr>,
}

impl Query {
    pub fn new(target: Id, request: RequestSpecific) -> Self {
        let mut table = RoutingTable::new().with_id(target);

        Self {
            target,
            request,
            table,
            // TODO: get timeout events from the socket.
            inflight_requests: Vec::new(),
            visited: HashSet::new(),
        }
    }

    pub fn is_done(&self) -> bool {
        self.inflight_requests.is_empty()
    }

    pub fn finish(&mut self) {
        self.visited.clear();
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

    pub fn closer_nodes(&mut self, tid: u16, from: SocketAddr, nodes: Vec<Node>) -> bool {
        if let Some(index) = self.inflight_requests.iter().position(|&x| x == tid) {
            self.inflight_requests.remove(index);

            for node in nodes {
                self.table.add(node);
            }

            return true;
        };

        false
    }

    pub fn next(&mut self, socket: &mut KrpcSocket) {
        let closest = self.table.closest(&self.target);

        if closest.is_empty() {
            return if self.is_done() { self.finish() } else { () };
        }

        for node in closest {
            if self.visited.contains(&node.address) {
                continue;
            };

            self.visit(socket, node.address);
        }
    }
}
