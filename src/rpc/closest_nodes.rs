use std::{convert::TryInto, rc::Rc};

use crate::{common::MAX_BUCKET_SIZE_K, Id, Node};

#[derive(Debug, Clone)]
/// Manage closest nodes found in a query.
///
/// Useful to estimate the Dht size.
pub struct ClosestNodes {
    target: Id,
    nodes: Vec<Rc<Node>>,
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

    pub fn nodes(&self) -> &[Rc<Node>] {
        &self.nodes
    }

    // === Public Methods ===

    pub fn add(&mut self, node: Rc<Node>) {
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

    /// Get the closest [K][MAX_BUCKET_SIZE_K] nodes or all the nodes until the
    /// expected distance of the Kth node, given a DHT size estimation.
    pub fn nodes_until_edk(&self, previous_dht_size_estimate: usize) -> &[Rc<Node>] {
        let mut until_edk = 0;

        // 20 / dht_size_estimate == expected_dk / ID space
        // so expected_dk = 20 * ID space / dht_size_estimate
        let expected_dk =
            (20.0 * u128::MAX as f64 / (previous_dht_size_estimate as f64 + 1.0)) as u128;

        for node in &self.nodes {
            let distance = distance(&self.target, node);

            if distance >= expected_dk {
                break;
            }

            until_edk += 1;
        }

        &self.nodes[0..until_edk.max(MAX_BUCKET_SIZE_K).min(self.nodes().len())]
    }

    /// An estimation of the Dht from the distribution of closest nodes
    /// responding to a query.
    ///
    /// [Read more](../../docs/dht_size_estimate.md)
    pub fn dht_size_estimate(&self) -> f64 {
        dht_size_estimate(
            self.nodes
                .iter()
                .take(MAX_BUCKET_SIZE_K)
                .map(|node| distance(&self.target, node)),
        )
    }
}

fn distance(target: &Id, node: &Node) -> u128 {
    let xor = node.id.xor(target);

    // Round up the lower 4 bytes to get a u128 from u160.
    u128::from_be_bytes(xor.as_bytes()[0..16].try_into().expect("infallible"))
}

fn dht_size_estimate<I>(distances: I) -> f64
where
    I: IntoIterator<Item = u128>,
{
    let mut sum = 0.0;
    let mut count = 0;

    // Ignoring the first node, as that gives the best result in simulations.
    for distance in distances {
        count += 1;

        sum += count as f64 * distance as f64;
    }

    if count == 0 {
        return 0.0;
    }

    let lsq_constant = (count * (count + 1) * (2 * count + 1) / 6) as f64;

    lsq_constant * u128::MAX as f64 / sum
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
            let node = Rc::new(Node::random());
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
        let unsecure = Rc::new(Node::random());
        let secure = Rc::new(Node {
            id: Id::from_str("5a3ce9c14e7a08645677bbd1cfe7d8f956d53256").unwrap(),
            address: (Ipv4Addr::new(21, 75, 31, 124), 0).into(),
            token: None,
            last_seen: Instant::now(),
        });

        let mut closest_nodes = ClosestNodes::new(*unsecure.id());

        closest_nodes.add(unsecure.clone());
        closest_nodes.add(secure.clone());

        assert_eq!(closest_nodes.nodes(), vec![secure, unsecure])
    }

    #[test]
    fn counter_vertical_sybil_attack() {
        let target = Id::random();
        let dht_size_estimate = 200;

        let mut closest_nodes = ClosestNodes::new(target);

        let target_bytes = target.as_bytes();

        for _ in 0..dht_size_estimate {
            closest_nodes.add(Node::random().into());
        }

        let mut sybil = ClosestNodes::new(target);

        for _ in 0..20 {
            let mut bytes = target_bytes.to_vec();
            bytes[18..].copy_from_slice(&Id::random().as_bytes()[18..]);
            let id = Id::from_bytes(bytes).unwrap();
            let node = Rc::new(Node::random().with_id(id));

            sybil.add(node.clone());
            closest_nodes.add(node);
        }

        let closest = closest_nodes.nodes_until_edk(dht_size_estimate);

        assert!((closest.len() - sybil.nodes().len()) > 10);
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
                    closest_nodes.add(node.clone().into())
                }
                for (_, node) in nodes.range(..target).rev().take(100) {
                    closest_nodes.add(node.clone().into())
                }

                let estimate = closest_nodes.dht_size_estimate();

                estimate as usize
            })
            .sum::<usize>()
            / lookups
    }
}
