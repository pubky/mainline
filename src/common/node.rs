//! Struct and implementation of the Node entry in the Kademlia routing table
use std::{
    fmt::{self, Debug, Formatter},
    net::SocketAddr,
    rc::Rc,
    time::{Duration, Instant},
};

use crate::common::Id;

/// The age of a node's last_seen time before it is considered stale and removed from a full bucket
/// on inserting a new node.
pub const STALE_TIME: Duration = Duration::from_secs(15 * 60);
const MIN_PING_BACKOFF_INTERVAL: Duration = Duration::from_secs(10);
pub const TOKEN_ROTATE_INTERVAL: Duration = Duration::from_secs(60 * 5);

#[derive(Clone, PartialEq)]
/// Node entry in Kademlia routing table
pub struct Node {
    pub(crate) id: Id,
    pub(crate) address: SocketAddr,
    pub(crate) token: Option<Vec<u8>>,
    pub(crate) last_seen: Instant,
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

    // === Getters ===

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub fn address(&self) -> &SocketAddr {
        &self.address
    }

    /// Creates a node with random Id for testing purposes.
    pub fn random() -> Node {
        Node {
            id: Id::random(),
            address: SocketAddr::from(([0, 0, 0, 0], 0)),
            token: None,
            last_seen: Instant::now(),
        }
    }

    #[cfg(any(test, feature = "__private_simulation"))]
    /// Create a node that is unique per `i` as it has a random Id and sets IP and port to `i`
    pub fn unique(i: usize) -> Node {
        use std::net::Ipv4Addr;

        Node::random().with_address(SocketAddr::from((Ipv4Addr::from_bits(i as u32), i as u16)))
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

    /// Node's token was received 5 minutes ago or less
    pub fn valid_token(&self) -> bool {
        self.last_seen.elapsed() <= TOKEN_ROTATE_INTERVAL
    }

    pub(crate) fn should_ping(&self) -> bool {
        self.last_seen.elapsed() > MIN_PING_BACKOFF_INTERVAL
    }

    /// Returns true if both nodes have the same ip and port
    pub fn same_adress(&self, other: &Self) -> bool {
        self.address == other.address
    }

    /// Returns true if both nodes have the same ip
    pub fn same_ip(&self, other: &Self) -> bool {
        self.address.ip() == other.address.ip()
    }

    /// Node [Id] is valid for its IP address.
    ///
    /// Check [BEP0042](https://www.bittorrent.org/beps/bep_0042.html).
    pub fn is_secure(&self) -> bool {
        self.id.is_valid_for_ip(&self.address.ip())
    }

    /// Returns true if Any of the existing nodes:
    ///  - Have the same IP as this node, And:
    ///     = The existing nodes is Not secure.
    ///     = The existing nodes is secure And shares the same first 21 bits.
    ///
    /// Effectively, allows only One non-secure node or Eight secure nodes from the same IP, in the routing table or ClosestNodes.
    pub(crate) fn already_exists(&self, nodes: &[Rc<Self>]) -> bool {
        nodes.iter().any(|existing| {
            self.same_ip(existing)
                && (!existing.is_secure()
                    || self.id().first_21_bits() == existing.id().first_21_bits())
        })
    }
}
