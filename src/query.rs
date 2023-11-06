use std::sync::mpsc::{self, Receiver, Sender};

use crate::common::{Id, Node};
use crate::messages::{Message, RequestSpecific};
use crate::routing_table::RoutingTable;
// use crate::rpc::Rpc;

#[derive(Debug)]
pub enum QueryType {
    FindNode { sender: Sender<Node> },
}

/// A query is an iterative process of concurrently sending a request to the closest known nodes to
/// the target, updating the routing table with closer nodes discovered in the responses, and
/// repeating this process until no closer nodes (that aren't already queried) are found.
#[derive(Debug)]
pub struct Query {
    target: Id,
    query_type: QueryType,
    pub request: RequestSpecific,
    pub table: RoutingTable,
    done: bool,
}

impl Query {
    pub fn new(target: Id, request: RequestSpecific) -> Self {
        println!("Adding QUERY! {:?}", target);

        let (sender, receiver) = mpsc::channel();

        let query_type = QueryType::FindNode { sender };

        let mut table = RoutingTable::new().with_id(target);

        Self {
            target,
            query_type,
            request,
            table,
            done: false,
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn restart(&mut self) {
        self.done = false;
    }

    pub fn finish(&mut self) {
        self.done = true;
    }
}
