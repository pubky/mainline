//! Kbuckets
use std::{
    fmt::{self, Debug, Formatter},
    slice::Iter,
    time::Instant,
};

use crate::common::Node;

/// K = the default maximum size of a k-bucket.
const DEFAULT_K: usize = 20;

/// Kbuckets are similar to LRU caches that checks and evicts unresponsive nodes,
/// without dropping any responsive nodes in the process.
pub struct KBucket {
    /// K (as in k-bucket) is the maximum number of nodes in a k-bucket.
    /// This controls the redundancy factor of the DHT client, the higher
    /// the more nodes we store (and thus lookup) values at.
    k: usize,
    /// Nodes in the k-bucket, sorted by the least recently seen.
    nodes: Vec<Node>,
    /// Keep track of the last time this bucket or any of its nodes were updated.
    last_updated: Instant,
}

impl KBucket {
    pub fn new() -> Self {
        KBucket {
            k: DEFAULT_K,
            nodes: Vec::with_capacity(DEFAULT_K),
            last_updated: Instant::now(),
        }
    }

    // === Options ===

    pub fn with_size(mut self, k: usize) -> Self {
        self.k = k;
        self.nodes = Vec::with_capacity(k);
        self
    }

    // === Public Methods ===

    pub fn add(&mut self, node: Node) -> bool {
        if self.nodes.contains(&node) {
            return false;
        }

        if self.nodes.len() < self.k {
            // TODO: revisit the ordering of this bucket!
            let index = match self.nodes.binary_search_by(|a| a.id.cmp(&node.id)) {
                // TODO: If we are not using BEP42, or if node changed its port,
                // we need to figure out what to do in this case, probably need to ping.
                Ok(existing_index) => existing_index,
                Err(insertion_index) => insertion_index,
            };

            self.nodes.insert(index, node);
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, Node> {
        self.nodes.iter()
    }

    pub fn get(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index)
    }
}

impl Debug for KBucket {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Row{{ nodes: {} }}", &self.nodes.len())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::common::Id;
    use std::net::SocketAddr;

    #[test]
    fn max_size() {
        let mut bucket = KBucket::new();
        for i in 0..bucket.k {
            bucket.add(Node {
                id: Id::random(),
                address: SocketAddr::from(([0, 0, 0, 0], 0)),
            });
        }

        assert_eq!(
            bucket.add(Node {
                id: Id::random(),

                address: SocketAddr::from(([0, 0, 0, 0], 0)),
            }),
            false
        )
    }
}
