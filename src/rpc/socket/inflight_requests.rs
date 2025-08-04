//! Inflight requests management for UDP socket layer.

use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct InflightRequest {
    pub transaction_id: u32,
    pub to: SocketAddrV4,
    pub sent_at: Instant,
}

#[derive(Debug)]
pub struct InflightRequests {
    requests: Vec<InflightRequest>,
    last_cleanup: Instant,
}

impl InflightRequests {
    pub fn new() -> Self {
        Self {
            requests: Vec::new(),
            last_cleanup: Instant::now(),
        }
    }

    /// Add a new inflight request
    pub fn add(&mut self, transaction_id: u32, to: SocketAddrV4) {
        self.requests.push(InflightRequest {
            transaction_id,
            to,
            sent_at: Instant::now(),
        });
    }

    /// Check if a transaction_id is still inflight
    pub fn contains(&self, transaction_id: u32) -> bool {
        self.requests
            .iter()
            .any(|req| req.transaction_id == transaction_id)
    }

    /// Remove inflight request by transaction_id if it exists and matches the address
    /// This maintains time-sorted order since we're removing from the middle
    pub fn remove(&mut self, transaction_id: u32, from: &SocketAddrV4) -> Option<InflightRequest> {
        if let Some(pos) = self
            .requests
            .iter()
            .position(|req| req.transaction_id == transaction_id)
        {
            let request = self.requests.remove(pos);
            if compare_socket_addr(&request.to, from) {
                Some(request)
            } else {
                // Put it back in the correct time-sorted position
                // Find where to insert to maintain time order
                let insert_pos = self
                    .requests
                    .iter()
                    .position(|req| req.sent_at > request.sent_at)
                    .unwrap_or(self.requests.len());
                self.requests.insert(insert_pos, request);
                None
            }
        } else {
            None
        }
    }

    /// Cleanup expired requests based on timeout
    pub fn cleanup(&mut self, timeout: Duration) {
        let now = Instant::now();

        // Only cleanup occasionally for performance
        if now.duration_since(self.last_cleanup) > Duration::from_millis(200) {
            self.last_cleanup = now;

            // Since Vec is time-sorted, find how many expired from front
            let mut expired_count = 0;
            for request in &self.requests {
                if request.sent_at.elapsed() <= timeout {
                    break; // All remaining are newer
                }
                expired_count += 1;
            }

            // Remove expired requests in one operation: O(n-k) instead of O(k×n)
            if expired_count > 0 {
                self.requests.drain(0..expired_count);
            }
        }
    }
}

impl Default for InflightRequests {
    fn default() -> Self {
        Self::new()
    }
}

// Same as SocketAddr::eq but ignores the ip if it is unspecified for testing reasons.
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

    #[test]
    fn test_add_and_contains() {
        let mut manager = InflightRequests::new();
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);

        assert!(!manager.contains(123));
        manager.add(123, addr);
        assert!(manager.contains(123));
    }

    #[test]
    fn test_remove_matching() {
        let mut manager = InflightRequests::new();
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);

        manager.add(123, addr);

        // Should find and remove matching request
        let removed = manager.remove(123, &addr);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().transaction_id, 123);
        assert!(!manager.contains(123));
    }

    #[test]
    fn test_remove_wrong_address() {
        let mut manager = InflightRequests::new();
        let addr1 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
        let addr2 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8081);

        manager.add(123, addr1);

        // Should not remove if address doesn't match
        let removed = manager.remove(123, &addr2);
        assert!(removed.is_none());
        assert!(manager.contains(123));
    }

    #[test]
    fn test_cleanup_expired() {
        let mut manager = InflightRequests::new();
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
        let timeout = Duration::from_secs(1);

        // Test scenario: mix of expired and valid requests
        let now = Instant::now();

        // Add expired requests (older than timeout)
        manager.requests.push(InflightRequest {
            transaction_id: 100,
            to: addr,
            sent_at: now - Duration::from_secs(3), // 3s old > 1s timeout
        });
        manager.requests.push(InflightRequest {
            transaction_id: 101,
            to: addr,
            sent_at: now - Duration::from_secs(2), // 2s old > 1s timeout
        });

        // Add valid requests (newer than timeout)
        manager.requests.push(InflightRequest {
            transaction_id: 102,
            to: addr,
            sent_at: now - Duration::from_millis(500), // 0.5s old < 1s timeout
        });
        manager.requests.push(InflightRequest {
            transaction_id: 103,
            to: addr,
            sent_at: now, // Just sent
        });

        // Force cleanup by setting last_cleanup to trigger condition
        manager.last_cleanup = now - Duration::from_millis(300);

        // Verify initial state
        assert_eq!(manager.requests.len(), 4);
        assert!(manager.contains(100));
        assert!(manager.contains(101));
        assert!(manager.contains(102));
        assert!(manager.contains(103));

        // Run cleanup
        manager.cleanup(timeout);

        // Verify results: expired removed, valid kept
        assert_eq!(manager.requests.len(), 2);
        assert!(!manager.contains(100)); // Expired - removed
        assert!(!manager.contains(101)); // Expired - removed
        assert!(manager.contains(102)); // Valid - kept
        assert!(manager.contains(103)); // Valid - kept

        // Verify time-sorted order is maintained
        assert!(manager.requests[0].transaction_id == 102);
        assert!(manager.requests[1].transaction_id == 103);
    }

    #[test]
    fn test_cleanup_edge_cases() {
        let mut manager = InflightRequests::new();
        let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
        let timeout = Duration::from_secs(1);

        // Test 1: Empty vec
        manager.cleanup(timeout);
        assert_eq!(manager.requests.len(), 0);

        // Test 2: All requests expired
        let old_time = Instant::now() - Duration::from_secs(5);
        manager.requests.push(InflightRequest {
            transaction_id: 1,
            to: addr,
            sent_at: old_time,
        });
        manager.requests.push(InflightRequest {
            transaction_id: 2,
            to: addr,
            sent_at: old_time,
        });

        manager.last_cleanup = Instant::now() - Duration::from_millis(300);
        manager.cleanup(timeout);
        assert_eq!(manager.requests.len(), 0);

        // Test 3: No requests expired
        let recent_time = Instant::now();
        manager.requests.push(InflightRequest {
            transaction_id: 3,
            to: addr,
            sent_at: recent_time,
        });
        manager.requests.push(InflightRequest {
            transaction_id: 4,
            to: addr,
            sent_at: recent_time,
        });

        manager.last_cleanup = Instant::now() - Duration::from_millis(300);
        let initial_len = manager.requests.len();
        manager.cleanup(timeout);
        assert_eq!(manager.requests.len(), initial_len); // No change
    }
}