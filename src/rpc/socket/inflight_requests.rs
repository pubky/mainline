use std::collections::HashMap;
use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct InflightRequest {
    pub transaction_id: u32,
    pub to: SocketAddrV4,
    pub sent_at: Instant,
}

#[derive(Debug)]
pub struct InflightRequests {
    // Vec stores the actual request data in insertion order, allowing efficient
    // iteration during cleanup operations. The Vec maintains the original order
    // which is useful for time-based operations and memory locality.
    requests: Vec<InflightRequest>,
    // HashMap provides O(1) lookup by transaction_id for contains() and remove()
    // operations. The usize value is the index into the Vec, enabling fast
    // random access to specific requests without scanning the entire Vec.
    index: HashMap<u32, usize>,
}

impl InflightRequests {
    pub fn new() -> Self {
        Self {
            requests: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Add a new inflight request O(1)
    pub fn add(&mut self, transaction_id: u32, to: SocketAddrV4) {
        let pos = self.requests.len();
        self.requests.push(InflightRequest {
            transaction_id,
            to,
            sent_at: Instant::now(),
        });
        self.index.insert(transaction_id, pos);
    }

    /// Check if a transaction_id is still inflight O(1)
    pub fn contains(&self, transaction_id: u32) -> bool {
        self.index.contains_key(&transaction_id)
    }

    /// Remove inflight request by transaction_id if it exists and matches the address
    /// O(1) average case
    pub fn remove(&mut self, transaction_id: u32, from: &SocketAddrV4) -> Option<InflightRequest> {
        if let Some(&pos) = self.index.get(&transaction_id) {
            if pos < self.requests.len() && self.requests[pos].transaction_id == transaction_id {
                let request = &self.requests[pos];
                if compare_socket_addr(&request.to, from) {
                    self.index.remove(&transaction_id);

                    // Mark as removed by setting transaction_id to 0
                    let mut removed = InflightRequest {
                        transaction_id: 0,
                        to: request.to,
                        sent_at: request.sent_at,
                    };
                    std::mem::swap(&mut removed, &mut self.requests[pos]);
                    removed.transaction_id = transaction_id; // Restore original id

                    return Some(removed);
                }
            }
        }
        None
    }

    /// Cleanup expired requests based on timeout
    /// O(n) scans all requests to remove expired ones and removed entries
    pub fn cleanup(&mut self, timeout: Duration) {
        let now = Instant::now();
        let cutoff = now - timeout;

        let mut new_requests = Vec::new();
        self.index.clear();

        for request in &self.requests {
            if request.transaction_id != 0 && request.sent_at > cutoff {
                let new_pos = new_requests.len();
                new_requests.push(request.clone());
                self.index.insert(request.transaction_id, new_pos);
            }
        }

        self.requests = new_requests;
    }

    /// Get the number of active requests (excluding removed ones)
    #[allow(dead_code)]
    pub fn active_count(&self) -> usize {
        self.index.len()
    }
}

// Same as SocketAddr eq but ignores the ip if it is unspecified for testing reasons.
fn compare_socket_addr(a: &SocketAddrV4, b: &SocketAddrV4) -> bool {
    if a.port() != b.port() {
        return false;
    }

    if a.ip().is_unspecified() {
        return true;
    }

    a.ip() == b.ip()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::time::Instant;

    #[test]
    fn test_basic_functionality() {
        let mut requests = InflightRequests::new();
        let addr1 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
        let addr2 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8081);

        // Test add and contains
        assert!(!requests.contains(123));
        requests.add(123, addr1);
        assert!(requests.contains(123));

        // Test remove matching address
        let removed = requests.remove(123, &addr1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().transaction_id, 123);
        assert!(!requests.contains(123));

        // Test remove wrong address
        requests.add(456, addr1);
        let removed = requests.remove(456, &addr2);
        assert!(removed.is_none());
        assert!(requests.contains(456));

        // Test cleanup with expired requests
        let timeout = Duration::from_secs(1);
        let now = Instant::now();
        let old_time = now - Duration::from_secs(2);

        // Add mix of expired and valid requests
        requests.requests.push(InflightRequest {
            transaction_id: 100,
            to: addr1,
            sent_at: old_time, // Expired
        });
        requests.index.insert(100, requests.requests.len() - 1);

        requests.requests.push(InflightRequest {
            transaction_id: 101,
            to: addr1,
            sent_at: now, // Valid
        });
        requests.index.insert(101, requests.requests.len() - 1);

        // Before cleanup
        assert!(requests.contains(456)); // Still there
        assert!(requests.contains(100)); // Expired but not cleaned yet
        assert!(requests.contains(101)); // Valid

        // After cleanup
        requests.cleanup(timeout);
        assert!(requests.contains(456)); // Still there
        assert!(!requests.contains(100)); // Expired removed
        assert!(requests.contains(101)); // Valid kept
    }

    #[test]
    fn test_performance_benchmark() {
        let mut manager = InflightRequests::new();
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
        let num_requests = 10_000;

        // Benchmark Add many requests
        let start = Instant::now();
        for i in 0..num_requests {
            manager.add(i, addr);
        }
        let add_duration = start.elapsed();
        println!(
            "Added {} requests in {:?} ({:.2} req/ms)",
            num_requests,
            add_duration,
            num_requests as f64 / add_duration.as_millis() as f64
        );

        // Benchmark Contains lookups (should be O(1))
        let start = Instant::now();
        for i in 0..num_requests {
            assert!(manager.contains(i));
        }
        let contains_duration = start.elapsed();
        println!(
            "Checked {} contains in {:?} ({:.2} req/ms)",
            num_requests,
            contains_duration,
            num_requests as f64 / contains_duration.as_millis() as f64
        );

        // Benchmark Remove requests (should be O(1) average)
        let start = Instant::now();
        let mut removed_count = 0;
        for i in 0..num_requests {
            if manager.remove(i, &addr).is_some() {
                removed_count += 1;
            }
        }
        let remove_duration = start.elapsed();
        println!(
            "Removed {} requests in {:?} ({:.2} req/ms)",
            removed_count,
            remove_duration,
            removed_count as f64 / remove_duration.as_millis() as f64
        );

        assert_eq!(removed_count, num_requests);
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn test_cleanup_performance() {
        let mut manager = InflightRequests::new();
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
        let num_requests = 10_000;
        let timeout = Duration::from_secs(1);

        // Add many expired requests
        let old_time = Instant::now() - Duration::from_secs(2);
        for i in 0..num_requests {
            manager.requests.push(InflightRequest {
                transaction_id: i,
                to: addr,
                sent_at: old_time,
            });
            manager.index.insert(i, i as usize);
        }

        // Benchmark cleanup
        let start = Instant::now();
        manager.cleanup(timeout);
        let cleanup_duration = start.elapsed();

        println!(
            "Cleaned up {} expired requests in {:?} ({:.2} req/ms)",
            num_requests,
            cleanup_duration,
            num_requests as f64 / cleanup_duration.as_millis() as f64
        );

        assert_eq!(manager.requests.len(), 0);
        assert_eq!(manager.active_count(), 0);
    }

    #[test]
    fn test_memory_cleanup_no_leaks() {
        let mut manager = InflightRequests::new();
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
        let timeout = Duration::from_secs(1);

        // Test 1 Add requests, remove some, verify removed entries and HashMap consistency
        for i in 0..1000 {
            manager.add(i, addr);
        }

        // Remove every other request manually (creates removed entries)
        for i in (0..1000).step_by(2) {
            manager.remove(i, &addr);
        }

        // Verify Vec has removed entries and HashMap only tracks active ones
        assert_eq!(manager.requests.len(), 1000); // Vec still has all entries including removed ones
        assert_eq!(manager.active_count(), 500); // HashMap only has active ones

        // Count removed entries
        let removed_entries = manager
            .requests
            .iter()
            .filter(|r| r.transaction_id == 0)
            .count();
        assert_eq!(removed_entries, 500); // Should have 500 removed entries

        // Test 2 Add some expired requests by inserting old entries
        let old_time = Instant::now() - Duration::from_secs(2);
        for i in 2000..2100 {
            manager.requests.push(InflightRequest {
                transaction_id: i,
                to: addr,
                sent_at: old_time, // Expired
            });
            manager.index.insert(i, manager.requests.len() - 1);
        }

        // Now we should have 500 active + 500 removed entries + 100 expired = 1100 total
        assert_eq!(manager.requests.len(), 1100);
        assert_eq!(manager.active_count(), 600); // 500 + 100 new ones

        // Test 3 Cleanup should remove both removed entries and expired requests
        manager.cleanup(timeout);

        // After cleanup only the 500 active (odd numbered) requests should remain
        assert_eq!(manager.requests.len(), 500); // Only active requests left
        assert_eq!(manager.active_count(), 500); // HashMap matches Vec

        // Verify no removed entries remain
        let removed_entries_after = manager
            .requests
            .iter()
            .filter(|r| r.transaction_id == 0)
            .count();
        assert_eq!(removed_entries_after, 0);

        // Verify HashMap index consistency
        for (vec_pos, request) in manager.requests.iter().enumerate() {
            assert_ne!(request.transaction_id, 0); // No removed entries
            assert_eq!(manager.index.get(&request.transaction_id), Some(&vec_pos));
        }

        // Test 4 Remove all remaining requests
        let remaining_requests: Vec<_> = manager
            .requests
            .iter()
            .map(|req| req.transaction_id)
            .collect();

        for tid in remaining_requests {
            manager.remove(tid, &addr);
        }

        assert_eq!(manager.active_count(), 0); // HashMap should be empty

        // Test 5 Final cleanup should remove all removed entries
        manager.cleanup(timeout);
        assert_eq!(manager.requests.len(), 0); // Vec should be empty
        assert_eq!(manager.active_count(), 0); // HashMap should be empty
    }
}
