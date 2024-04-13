//! A struct to iterate on incoming responses for a query.
use bytes::Bytes;
use flume::Sender;
use std::net::SocketAddr;

use super::{Id, MutableItem, Node};

#[derive(Clone, Debug)]
pub enum Response {
    Peer(SocketAddr),
    Immutable(Bytes),
    Mutable(MutableItem),
}

#[derive(Clone, Debug)]
pub enum ResponseSender {
    Peer(Sender<SocketAddr>),
    Mutable(Sender<MutableItem>),
    Immutable(Sender<Bytes>),
}

#[derive(Clone, Debug)]
pub struct StoreQueryMetdata {
    target: Id,
    stored_at: Vec<Id>,
    closest_nodes: Vec<Node>,
}

impl StoreQueryMetdata {
    pub fn new(target: Id, closest_nodes: Vec<Node>, stored_at: Vec<Id>) -> Self {
        Self {
            target,
            closest_nodes,
            stored_at,
        }
    }

    /// Return the target (or info_hash) for this query.
    pub fn target(&self) -> Id {
        self.target
    }

    /// Return the set of nodes that confirmed storing the value.
    pub fn stored_at(&self) -> Vec<&Node> {
        self.closest_nodes
            .iter()
            .filter(|node| self.stored_at.contains(&node.id))
            .collect()
    }

    /// Return closest nodes. Useful to repeat the store operation without repeating the lookup.
    pub fn closest_nodes(&self) -> Vec<Node> {
        self.closest_nodes.clone()
    }
}
