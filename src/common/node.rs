//! Struct and implementation of the Node entry in the Kademlia routing table
use std::net::SocketAddr;

use crate::common::Id;

#[derive(Debug, Clone, PartialEq)]
/// Node entry in Kademlia routing table
pub struct Node {
    pub id: Id,
    pub address: SocketAddr,
}

impl Node {
    /// Creates a new Node from an id and socket address.
    pub fn new(id: Id, address: SocketAddr) -> Node {
        Node { id, address }
    }
}
