use std::vec::IntoIter;

use crate::{Id, Node};

#[derive(Debug, Clone)]
pub struct ClosestNodes {
    target: Id,
    nodes: Vec<Node>,
}

impl ClosestNodes {
    pub fn new(target: Id) -> Self {
        Self {
            target,
            nodes: Vec::with_capacity(200),
        }
    }

    // === Getters ===

    pub fn target(&self) -> Id {
        self.target
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    // === Public Methods ===

    pub fn add(&mut self, node: Node) {
        match self.nodes.binary_search_by(|item| item.id.cmp(&node.id)) {
            Ok(pos) | Err(pos) => self.nodes.insert(pos, node),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// An estimation of the Dht from the distribution of closest nodes
    /// responding to a query.
    ///
    /// In order to get an accurate calculation of the Dht size, you should take
    /// as many lookups (at uniformly disrtibuted target) as you can,
    /// and calculate the average of the estimations based on their responding nodes.
    ///
    /// # Explanation
    ///
    /// Consider a Dht with a 4 bit key space.
    /// Then we can map nodes in that keyspace by their distance to a given target of a lookup.
    ///
    /// Assuming a random but uniform distribution of nodes (which can be measured independently),
    /// you should see nodes distributed somewhat like this:
    ///
    /// ```md
    ///              (1)    (2)                  (3)    (4)           (5)           (6)           (7)      (8)       
    /// |------|------|------|------|------|------|------|------|------|------|------|------|------|------|------|
    /// 0      1      2      3      4      5      6      7      8      9      10     11     12     13     14     15
    /// ```
    ///
    /// So if you make a lookup and optained this partial view of the network:
    /// ```md
    ///              (1)    (2)                  (3)                                (4)                  (5)       
    /// |------|------|------|------|------|------|------|------|------|------|------|------|------|------|------|
    /// 0      1      2      3      4      5      6      7      8      9      10     11     12     13     14     15
    /// ```
    ///
    /// Note: you see exponentially less further nodes than closer ones, which is what you should expect from how
    /// the routing table works.
    ///
    /// Seeing one node at distance (d1=2), suggests that the routing table might contain 8 nodes,
    /// since its full length is 8 times (d1).
    ///
    /// Similarily, seeing two nodes at (d2=3), suggests that the routing table might contain ~11
    /// nodes, since the key space is more than (d2).
    ///
    /// If we repeat this estimation for as many nodes as the routing table's `k` bucket size,
    /// and take their average, we get a more accurate estimation of the dht.
    ///
    /// ## Formula
    ///
    /// The estimated number of Dht size, at each distance `di`, is `en_i = i * d_max / di` where `i` is the
    /// count of nodes discovered until this distance and `d_max` is the size of the key space.
    ///
    /// The final Dht size estimation is the average of `en_1 + en_2 + .. + en_n`
    ///
    /// Read more at [A New Method for Estimating P2P Network Size](https://eli.sohl.com/2020/06/05/dht-size-estimation.html#fnref:query-count)
    pub fn dht_size_estimate(&self) -> usize {
        if self.is_empty() {
            return 0;
        };

        self.nodes
            .iter()
            .map(|n| n.id)
            .enumerate()
            .fold(0, |mut sum, (idx, id)| {
                let intervals = id.keyspace_intervals(self.target);
                let estimated_n = intervals.saturating_mul(idx + 1);

                sum += estimated_n;
                sum
            })
            / self.nodes.len()
    }
}

impl IntoIterator for ClosestNodes {
    type Item = Node;
    type IntoIter = IntoIter<Node>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.into_iter()
    }
}

impl<'a> IntoIterator for &'a ClosestNodes {
    type Item = &'a Node;
    type IntoIter = std::slice::Iter<'a, Node>;

    fn into_iter(self) -> Self::IntoIter {
        self.nodes.iter()
    }
}
