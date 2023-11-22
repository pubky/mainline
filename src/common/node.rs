//! Struct and implementation of the Node entry in the Kademlia routing table
use std::{
    fmt::{self, Debug, Formatter},
    net::SocketAddr,
    time::{Duration, Instant},
};

use crate::common::Id;

/// The age of a node's last_seen time before it is considered stale and removed from a full bucket
/// on inserting a new node.
pub const STALE_TIME: Duration = Duration::from_secs(15 * 60);
const MIN_PING_BACKOFF_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, PartialEq)]
/// Node entry in Kademlia routing table
pub struct Node {
    pub id: Id,
    pub address: SocketAddr,
    pub token: Option<Vec<u8>>,
    last_seen: Instant,
}

impl Debug for Node {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Node")
            .field("id", &self.id)
            .field("address", &self.address)
            .field("last_seen", &self.last_seen.elapsed().as_secs())
            .finish()
    }
}

impl Node {
    /// Creates a new Node from an id and socket address.
    pub fn new(id: Id, address: SocketAddr) -> Node {
        Node {
            id,
            address,
            token: None,
            last_seen: Instant::now(),
        }
    }

    /// Creates a random node for testing purposes.
    #[cfg(test)]
    pub fn random() -> Node {
        Node {
            id: Id::random(),
            address: SocketAddr::from(([0, 0, 0, 0], 0)),
            token: None,
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

    pub fn with_token(mut self, token: Vec<u8>) -> Self {
        self.token = Some(token);
        self
    }

    /// Node is last seen more than a threshold ago.
    pub fn is_stale(&self) -> bool {
        self.last_seen.elapsed() > STALE_TIME
    }

    pub(crate) fn _should_ping(&self) -> bool {
        self.last_seen.elapsed() > MIN_PING_BACKOFF_INTERVAL
    }

    /// Returns true if both nodes have the same id and address
    pub fn same_adress(&self, other: &Self) -> bool {
        self.address == other.address
    }
}
