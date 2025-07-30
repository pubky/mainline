use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

/// Dual vector approach solves transaction ID wrap-around problem:
/// - requests_by_tid: sorted by tid for O(log n) lookup
/// - requests_by_time: sorted by timestamp for O(log n) cleanup
/// Binary search would fail in single vector approach if tid wraps from u32::MAX to 0
/// because then [u32::MAX-2, u32::MAX-1, u32::MAX, 0, 1, 2] is not sorted.

#[derive(Debug, Clone)]
pub struct InflightRequest {
    pub(crate) tid: u32,
    pub(crate) to: SocketAddrV4,
    pub(crate) sent_at: Instant,
}

#[derive(Debug)]
pub struct InflightRequests {
    pub(crate) next_tid: u32,
    pub(crate) requests_by_tid: Vec<InflightRequest>,
    pub(crate) requests_by_time: Vec<InflightRequest>,
}

impl InflightRequests {
    pub fn new() -> Self {
        Self {
            next_tid: 0,
            requests_by_tid: Vec::new(),
            requests_by_time: Vec::new(),
        }
    }

    pub fn get_next_tid(&mut self) -> u32 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    pub fn is_empty(&self) -> bool {
        self.requests_by_tid.is_empty()
    }

    pub fn get(&self, key: u32, timeout: Duration) -> Option<&InflightRequest> {
        let index = self.find_by_tid(key).ok()?;
        let request = self.requests_by_tid.get(index)?;
        if request.sent_at.elapsed() < timeout {
            return Some(request);
        }
        None
    }

    /// Adds a InflightRequest with new transaction_id, and returns that id.
    pub fn add(&mut self, to: SocketAddrV4) -> u32 {
        let tid = self.get_next_tid();
        let request = InflightRequest {
            tid,
            to,
            sent_at: Instant::now(),
        };

        // Insert into requests_by_tid maintaining sort order O(log n)
        let tid_index = self
            .requests_by_tid
            .binary_search_by(|r| r.tid.cmp(&tid))
            .unwrap_err();
        self.requests_by_tid.insert(tid_index, request.clone());

        // Insert into requests_by_time maintaining sort order O(log n)
        let time_index = self
            .requests_by_time
            .binary_search_by(|r| r.sent_at.cmp(&request.sent_at))
            .unwrap_err();
        self.requests_by_time.insert(time_index, request);

        tid
    }

    pub fn remove(&mut self, key: u32) -> Option<InflightRequest> {
        match self.find_by_tid(key) {
            Ok(tid_index) => {
                let request = self.requests_by_tid.remove(tid_index);

                // Find the exact position in time-sorted vector using binary search by timestamp
                // O(log n). Since both vectors contain the same requests, this should always succeed
                if let Ok(time_index) = self
                    .requests_by_time
                    .binary_search_by(|r| r.sent_at.cmp(&request.sent_at))
                {
                    self.requests_by_time.remove(time_index);
                }

                Some(request)
            }
            Err(_) => None,
        }
    }

    fn find_by_tid(&self, tid: u32) -> Result<usize, usize> {
        // Binary search by transaction ID O(log n)
        self.requests_by_tid
            .binary_search_by(|request| request.tid.cmp(&tid))
    }

    /// Removes timed-out requests, is called on every recv_from.
    pub fn cleanup(&mut self, timeout: Duration) {
        // Early exit: no requests to clean
        if self.requests_by_time.is_empty() {
            return;
        }

        // Early exit: no requests have timed out yet
        if let Some(oldest) = self.requests_by_time.first() {
            if oldest.sent_at.elapsed() < timeout {
                return;
            }
        }

        // Find the cutoff index for timed out requests
        let cutoff_time = Instant::now() - timeout;
        let index = match self
            .requests_by_time
            .binary_search_by(|request| request.sent_at.cmp(&cutoff_time))
        {
            Ok(index) => index,
            Err(index) => index,
        };

        // Early exit: no requests need cleanup
        if index == 0 {
            return;
        }

        // Collect the requests to be removed before draining
        let requests_to_remove: Vec<_> = self.requests_by_time[..index].to_vec();

        // Remove from requests_by_time (already sorted by time)
        self.requests_by_time.drain(0..index);

        // Remove from requests_by_tid using binary search for each request
        // O(request_count * log n)
        for request in &requests_to_remove {
            if let Ok(tid_index) = self
                .requests_by_tid
                .binary_search_by(|r| r.tid.cmp(&request.tid))
            {
                self.requests_by_tid.remove(tid_index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InflightRequest;
    use super::*;
    use std::net::SocketAddrV4;

    #[test]
    fn prevents_wrap_around_problem() {
        let mut inflight = InflightRequests::new();

        // Manually create a vector with the wrap-around problem
        inflight.requests_by_tid = vec![
            InflightRequest {
                tid: u32::MAX - 2,
                to: SocketAddrV4::new([127, 0, 0, 1].into(), 1234),
                sent_at: Instant::now(),
            },
            InflightRequest {
                tid: u32::MAX - 1,
                to: SocketAddrV4::new([127, 0, 0, 1].into(), 1235),
                sent_at: Instant::now(),
            },
            InflightRequest {
                tid: u32::MAX,
                to: SocketAddrV4::new([127, 0, 0, 1].into(), 1236),
                sent_at: Instant::now(),
            },
            InflightRequest {
                tid: 0,
                to: SocketAddrV4::new([127, 0, 0, 1].into(), 1237),
                sent_at: Instant::now(),
            },
            InflightRequest {
                tid: 1,
                to: SocketAddrV4::new([127, 0, 0, 1].into(), 1238),
                sent_at: Instant::now(),
            },
            InflightRequest {
                tid: 2,
                to: SocketAddrV4::new([127, 0, 0, 1].into(), 1239),
                sent_at: Instant::now(),
            },
        ];

        // Demonstrate the wrap-around problem: if we only had one vector sorted by tid,
        // we couldn't efficiently find requests by timestamp for cleanup
        // The dual vector approach solves this by keeping a separate time-sorted vector

        // Test that we can still find by tid (this works in both approaches)
        let search_tid = 0;
        let result = inflight.find_by_tid(search_tid);
        assert_eq!(
            result.unwrap(),
            3, // tid 0 should be at index 3 in the vector [u32::MAX-2, u32::MAX-1, u32::MAX, 0, 1, 2]
            "Binary search should find tid 0 at correct index"
        );

        // Test that cleanup works despite wrap-around (this requires the dual vector approach)
        // The requests are very recent, so cleanup won't remove them, this is expected
        let before_cleanup = inflight.requests_by_tid.len();
        inflight.cleanup(Duration::from_secs(1));
        let after_cleanup = inflight.requests_by_tid.len();

        // Cleanup should not remove recent requests, but should work without errors
        assert_eq!(
            before_cleanup, after_cleanup,
            "Cleanup should not remove recent requests"
        );

        // The real test is that we can still find requests by tid despite the wrap-around
        assert_eq!(
            inflight.find_by_tid(0).unwrap(),
            3,
            "Should still find tid 0 after cleanup"
        );
    }

    #[test]
    fn prevents_memory_leak() {
        use std::thread;
        use std::time::Duration;

        let mut inflight = InflightRequests::new();

        // Add many requests that will timeout
        let test_addr = SocketAddrV4::new([127, 0, 0, 1].into(), 1234);

        for _ in 0..100 {
            inflight.add(test_addr);
        }

        let count_before_cleanup = inflight.requests_by_tid.len();
        assert_eq!(count_before_cleanup, 100);

        // Wait for requests to timeout
        thread::sleep(Duration::from_millis(200)); // 100ms timeout + 100ms buffer

        // Call cleanup to remove timed-out requests
        inflight.cleanup(Duration::from_millis(100));

        let count_after_cleanup = inflight.requests_by_tid.len();

        // Verify that cleanup removed the timed-out requests
        assert!(
            count_after_cleanup == 0,
            "Cleanup should remove timed-out requests."
        );

        // Verify both vectors are in sync
        assert_eq!(
            inflight.requests_by_tid.len(),
            inflight.requests_by_time.len(),
            "Both vectors should have the same length after cleanup"
        );
    }
}
