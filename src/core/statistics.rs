//! DHT statistics and size estimation.

use std::num::NonZeroUsize;

use lru::LruCache;

use crate::common::{Id, Node};
use crate::core::iterative_query::IterativeQuery;

const MAX_CACHED_ITERATIVE_QUERIES: usize = 1000;

/// Statistics about the DHT network
#[derive(Debug)]
pub struct DhtStatistics {
    /// Sum of DHT size estimates from all queries
    dht_size_estimates_sum: f64,

    /// Sum of DHT size estimates based on responding nodes
    responders_based_dht_size_estimates_sum: f64,

    /// Count of queries used for responders-based estimates
    responders_based_dht_size_estimates_count: usize,

    /// Sum of subnet diversity metrics
    subnets_sum: usize,

    /// Cache of completed queries with their results
    cached_queries: LruCache<Id, CachedIterativeQuery>,
}

#[derive(Debug)]
struct CachedIterativeQuery {
    closest_responding_nodes: Box<[Node]>,
    dht_size_estimate: f64,
    responders_dht_size_estimate: f64,
    subnets: u8,
    is_find_node: bool,
}

impl DhtStatistics {
    /// Create new DHT statistics tracker
    pub fn new() -> Self {
        DhtStatistics {
            dht_size_estimates_sum: 0.0,
            responders_based_dht_size_estimates_sum: 1_000_000.0,
            responders_based_dht_size_estimates_count: 0,
            subnets_sum: 20,
            cached_queries: LruCache::new(
                NonZeroUsize::new(MAX_CACHED_ITERATIVE_QUERIES).expect("valid non-zero"),
            ),
        }
    }

    /// Get DHT size estimate based on all queries
    pub fn dht_size_estimate(&self) -> (usize, f64) {
        let sample_count = self.cached_queries.len();
        if sample_count == 0 {
            return (0, 0.0);
        }

        let normal = self.dht_size_estimates_sum as usize / sample_count;

        // Standard deviation calculation
        let std_dev = 0.281 * (sample_count as f64).powf(-0.529);

        (normal, std_dev)
    }

    /// Get DHT size estimate based on responding nodes only
    pub fn responders_based_dht_size_estimate(&self) -> usize {
        self.responders_based_dht_size_estimates_sum as usize
            / self.responders_based_dht_size_estimates_count.max(1)
    }

    /// Get average subnet diversity
    pub fn average_subnets(&self) -> usize {
        self.subnets_sum / self.cached_queries.len().max(1)
    }

    /// Cache a completed query
    pub fn cache_query(&mut self, query: &IterativeQuery, closest_responding_nodes: &[Node]) {
        let closest = query.closest();
        if closest.nodes().is_empty() {
            // Node is offline
            return;
        }

        // Evict LRU if at capacity
        if self.cached_queries.len() >= MAX_CACHED_ITERATIVE_QUERIES {
            if let Some((_, old_query)) = self.cached_queries.pop_lru() {
                self.decrement_stats(&old_query);
            }
        }

        let responders = query.responders();

        let dht_size_estimate = closest.dht_size_estimate();
        let responders_dht_size_estimate = responders.dht_size_estimate();
        let subnets_count = closest.subnets_count();

        let is_find_node = query.is_find_node();

        let cached = CachedIterativeQuery {
            closest_responding_nodes: closest_responding_nodes.into(),
            dht_size_estimate,
            responders_dht_size_estimate,
            subnets: subnets_count,
            is_find_node,
        };

        // Update sums before inserting
        if let Some(old) = self.cached_queries.put(query.target(), cached) {
            self.decrement_stats(&old);
        }

        // Increment with new values
        self.dht_size_estimates_sum += dht_size_estimate;
        self.responders_based_dht_size_estimates_sum += responders_dht_size_estimate;
        self.subnets_sum += subnets_count as usize;

        if !is_find_node {
            self.responders_based_dht_size_estimates_count += 1;
        }
    }

    /// Get cached query results and update LRU
    pub fn get_cached_query(&mut self, target: &Id) -> Option<&[Node]> {
        self.cached_queries
            .get(target)
            .map(|q| q.closest_responding_nodes.as_ref())
    }

    fn decrement_stats(&mut self, query: &CachedIterativeQuery) {
        self.dht_size_estimates_sum -= query.dht_size_estimate;
        self.responders_based_dht_size_estimates_sum -= query.responders_dht_size_estimate;
        self.subnets_sum -= query.subnets as usize;

        if !query.is_find_node {
            self.responders_based_dht_size_estimates_count -= 1;
        }
    }
}

impl Default for DhtStatistics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::common::GetValueRequestArguments;
    use crate::common::{Id, Node};
    use crate::core::iterative_query::{GetRequestSpecific, IterativeQuery};

    use super::{DhtStatistics, MAX_CACHED_ITERATIVE_QUERIES};

    fn build_query(target: Id, node: Node) -> IterativeQuery {
        let mut query = IterativeQuery::new(
            Id::random(),
            target,
            GetRequestSpecific::GetValue(GetValueRequestArguments {
                target,
                seq: None,
                salt: None,
            }),
        );
        query.add_candidate(node.clone());
        query.add_responding_node(node);
        query
    }

    #[test]
    fn dht_size_estimate_empty_is_zero() {
        let stats = DhtStatistics::new();
        assert_eq!(stats.dht_size_estimate(), (0, 0.0));
    }

    #[test]
    fn cache_query_does_not_evict_on_empty_closest() {
        let mut stats = DhtStatistics::new();

        for i in 0..MAX_CACHED_ITERATIVE_QUERIES {
            let target = Id::random();
            let node = Node::unique(i);
            let query = build_query(target, node);
            let closest = query.closest().nodes();
            stats.cache_query(&query, closest);
        }

        let (_, std_dev_before) = stats.dht_size_estimate();

        let empty_target = Id::random();
        let empty_query = IterativeQuery::new(
            Id::random(),
            empty_target,
            GetRequestSpecific::GetValue(GetValueRequestArguments {
                target: empty_target,
                seq: None,
                salt: None,
            }),
        );
        stats.cache_query(&empty_query, &[]);

        let (_, std_dev_after) = stats.dht_size_estimate();
        assert!((std_dev_before - std_dev_after).abs() < f64::EPSILON);
    }
}
