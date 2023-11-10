//! Struct and implementation of the Node entry in the Kademlia routing table
use std::{
    fmt::{self, Debug, Formatter},
    net::SocketAddr,
    time::{Duration, Instant},
};

use crate::common::Id;

/// The age of a node's last_seen time before it is considered stale and removed from a full bucket
/// on inserting a new node.
const STALE_TIME: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, PartialOrd, Eq, Ord)]
/// Node entry in Kademlia routing table
pub struct Node {
    pub id: Id,
    pub address: SocketAddr,
    last_seen: Instant,
}

impl Debug for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Node {:?} {:?}", self.id, self.address)
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.address == other.address
    }
}

impl Node {
    /// Creates a new Node from an id and socket address.
    pub fn new(id: Id, address: SocketAddr) -> Node {
        Node {
            id,
            address,
            last_seen: Instant::now(),
        }
    }

    /// Creates a random node for testing purposes.
    pub fn random() -> Node {
        Node {
            id: Id::random(),
            address: SocketAddr::from(([0, 0, 0, 0], 0)),
            last_seen: Instant::now(),
        }
    }

    pub fn with_id(mut self, id: Id) -> Self {
        self.id = id;
        self
    }

    pub fn with_address(mut self, address: SocketAddr) -> Self {
        self.address = address;
        self
    }

    /// Node is last seen more than a threshold ago.
    pub fn is_stale(&self) -> bool {
        self.last_seen.elapsed() > STALE_TIME
    }
}
