use std::{convert::TryInto, vec::IntoIter};

use crate::{common::MAX_BUCKET_SIZE_K, Id, Node};

#[derive(Debug, Clone)]
/// Manage closest nodes found in a query.
///
/// Useful to estimate the Dht size.
pub struct ClosestNodes {
    target: Id,
    nodes: Vec<Node>,
    pub(crate) dht_size_estimate: Option<f64>,
}

impl ClosestNodes {
    pub fn new(target: Id) -> Self {
        Self {
            target,
            nodes: Vec::with_capacity(200),
            dht_size_estimate: None,
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

        if let Err(pos) = self.nodes.binary_search_by(|prope| {
            if prope.is_secure() && !node.is_secure() {
                std::cmp::Ordering::Less
            } else if !prope.is_secure() && node.is_secure() {
                std::cmp::Ordering::Greater
            } else if prope.id == node.id {
                std::cmp::Ordering::Equal
            } else {
                prope.id.xor(&self.target).cmp(&seek)
            }
        }) {
            self.nodes.insert(pos, node)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Remove all nodes too close according to what we know about the Dht size.
    ///
    /// Make sure that we retain minimum [MAX_BUCKET_SIZE_K] even if some are too close to the
    /// target.
    pub(crate) fn remove_sybil(&mut self, previous_dht_size_estimate: usize, std_dev: f64) {
        // TODO: Write a unit test to prove we are ignoring Sybil.
        let minimum_ed1 =
            (1.0 / (previous_dht_size_estimate as f64 + 1.0)) / (1.0 + (std_dev * 2.0));

        let minimum_index = self.nodes.len().saturating_sub(MAX_BUCKET_SIZE_K);

        let distances = self
            .nodes
            .iter()
            .map(|node| distance(&self.target, node))
            .enumerate()
            .filter(|(i, d)| {
                !(
                    // Is sybil
                    *d < minimum_ed1
                    // Is not necessary
                    && *i <  minimum_index
                )
            })
            .map(|(_, d)| d)
            .take(MAX_BUCKET_SIZE_K);

        let (estimate, count) = dht_size_estimate(distances);

        self.dht_size_estimate = Some(estimate);
        self.nodes.truncate(count);
    }

    /// An estimation of the Dht from the distribution of closest nodes
    /// responding to a query.
    ///
    /// [Read more](../../docs/dht_size_estimate.md)
    pub fn dht_size_estimate(&self) -> f64 {
        self.dht_size_estimate.unwrap_or(
            dht_size_estimate(
                self.nodes
                    .iter()
                    .take(MAX_BUCKET_SIZE_K)
                    .map(|node| distance(&self.target, node)),
            )
            .0,
        )
    }
}

fn distance(target: &Id, node: &Node) -> f64 {
    let xor = node.id.xor(target);

    // Round up the lower 4 bytes to get a u128 from u160.
    u128::from_be_bytes(xor.as_bytes()[0..16].try_into().expect("infallible")) as f64
}

fn dht_size_estimate<I>(distances: I) -> (f64, usize)
where
    I: IntoIterator<Item = f64>,
{
    let mut sum = 0.0;
    let mut count = 0;

    // Ignoring the first node, as that gives the best result in simulations.
    for distance in distances {
        count += 1;

        sum += count as f64 * distance;
    }

    if count == 0 {
        return (0.0, 0);
    }

    let lsq_constant = (count * (count + 1) * (2 * count + 1) / 6) as f64;

    let estimate = lsq_constant * u128::MAX as f64 / sum;

    (estimate, count)
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
    use std::{collections::BTreeMap, net::Ipv4Addr, str::FromStr, time::Instant};

    use super::*;

    #[test]
    fn add_sorted_by_id() {
        let target = Id::random();

        let mut closest_nodes = ClosestNodes::new(target);

        for _ in 0..100 {
            let node = Node::random();
            closest_nodes.add(node.clone());
            closest_nodes.add(node);
        }

        assert_eq!(closest_nodes.nodes().len(), 100);

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
    fn order_by_secure_id() {
        let unsecure = Node::random();
        let secure = Node {
            id: Id::from_str("5a3ce9c14e7a08645677bbd1cfe7d8f956d53256").unwrap(),
            address: (Ipv4Addr::new(21, 75, 31, 124), 0).into(),
            token: None,
            last_seen: Instant::now(),
        };

        let mut closest_nodes = ClosestNodes::new(*unsecure.id());

        closest_nodes.add(unsecure.clone());
        closest_nodes.add(secure.clone());

        assert_eq!(closest_nodes.nodes(), vec![secure, unsecure])
    }

    #[test]
    fn simulation() {
        let lookups = 4;
        let acceptable_margin = 0.2;
        let sims = 10;
        let dht_size = 2500 as f64;

        let mean = (0..sims)
            .into_iter()
            .map(|_| simulate(dht_size as usize, lookups) as f64)
            .sum::<f64>()
            / (sims as f64);

        let margin = (mean - dht_size).abs() / dht_size;

        assert!(margin <= acceptable_margin);
    }

    fn simulate(dht_size: usize, lookups: usize) -> usize {
        let mut nodes = BTreeMap::new();
        for _ in 0..dht_size {
            let node = Node::random();
            nodes.insert(node.id, node);
        }

        (0..lookups)
            .map(|_| {
                let target = Id::random();

                let mut closest_nodes = ClosestNodes::new(target);

                for (_, node) in nodes.range(target..).take(100) {
                    closest_nodes.add(node.clone())
                }
                for (_, node) in nodes.range(..target).rev().take(100) {
                    closest_nodes.add(node.clone())
                }

                let estimate = closest_nodes.dht_size_estimate();

                estimate as usize
            })
            .sum::<usize>()
            / lookups
    }
}
