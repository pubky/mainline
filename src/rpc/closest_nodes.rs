use std::{convert::TryInto, vec::IntoIter};

use crate::{common::MAX_BUCKET_SIZE_K, Id, Node};

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
        let seek = node.id.xor(&self.target);

        match self.nodes.binary_search_by(|prope| {
            if prope.id == node.id {
                std::cmp::Ordering::Equal
            } else {
                prope.id.xor(&self.target).cmp(&seek)
            }
        }) {
            Err(pos) => self.nodes.insert(pos, node),
            _ => {}
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

        let mut sum: usize = 0;
        let mut count = 0;

        for node in &self.nodes {
            if count >= MAX_BUCKET_SIZE_K {
                break;
            }

            count += 1;

            let xor = node.id.xor(&self.target);

            // Round up the lower 4 bytes to get a u128 from u160.
            let distance =
                u128::from_be_bytes(xor.as_bytes()[0..16].try_into().expect("infallible")) + 1;

            let intervals = (u128::MAX / distance) as usize;
            let estimated_n = intervals.saturating_mul(count);

            sum = sum.saturating_add(estimated_n);
        }

        (sum / count) as usize
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn add() {
        let target = Id::random();

        let mut closest_nodes = ClosestNodes::new(target);

        for _ in 0..10 {
            let node = Node::random();
            closest_nodes.add(node.clone());
            closest_nodes.add(node);
        }

        assert_eq!(closest_nodes.nodes().len(), 10);

        let distances = closest_nodes
            .nodes()
            .iter()
            .map(|n| n.id.distance(&target))
            .collect::<Vec<_>>();

        let mut sorted = distances.clone();
        sorted.sort();

        assert_eq!(sorted, distances);
    }

    #[test]
    fn simulation() {
        let lookups = 10;
        let acceptable_margin = 0.6;

        let tests = [2500, 25000, 250000];

        for dht_size in tests {
            let estimate = simulate(dht_size, lookups) as f64;

            let margin = (estimate - (dht_size as f64)).abs() / dht_size as f64;

            assert!(margin <= acceptable_margin);
        }
    }

    fn simulate(dht_size: usize, lookups: usize) -> usize {
        let mut nodes = BTreeMap::new();

        // Bootstrap
        for _ in 0..dht_size {
            let node = Node::random();
            nodes.insert(node.id, node);
        }

        let mut estimates = vec![];

        for _ in 0..lookups.min(dht_size) {
            let target = Id::random();

            let mut closest_nodes = ClosestNodes::new(target);

            for (_, node) in nodes.range(target..).take(20) {
                closest_nodes.add(node.clone());
            }
            for (_, node) in nodes.range(target..).rev().take(20) {
                closest_nodes.add(node.clone());
            }

            let estimate = closest_nodes.dht_size_estimate();

            estimates.push(estimate)
        }

        estimates.iter().sum::<usize>() / estimates.len()
    }
}
