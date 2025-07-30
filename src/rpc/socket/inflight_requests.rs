use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

/// Dual vector approach solves transaction ID wrap-around problem:
/// - requests_by_tid: sorted by tid for O(log n) lookup
/// - requests_by_time: sorted by timestamp for O(log n) cleanup
/// Without this, binary search fails when tid wraps from u32::MAX to 0.

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
    estimated_rtt: Duration,
    deviation_rtt: Duration,
}

impl InflightRequests {
    pub fn new() -> Self {
        Self {
            next_tid: 0,
            requests_by_tid: Vec::new(),
            requests_by_time: Vec::new(),
            estimated_rtt: Duration::from_secs(5),
            deviation_rtt: Duration::from_secs(0),
        }
    }

    /// Increments self.next_tid and returns the previous value.
    pub fn get_next_tid(&mut self) -> u32 {
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    pub fn request_timeout(&self) -> Duration {
        self.estimated_rtt + self.deviation_rtt.mul_f64(4.0)
    }

    pub fn is_empty(&self) -> bool {
        self.requests_by_tid.is_empty()
    }

    pub fn estimated_rtt(&self) -> Duration {
        self.estimated_rtt
    }

    pub fn get(&self, key: u32) -> Option<&InflightRequest> {
        let index = self.find_by_tid(key).ok()?;
        let request = self.requests_by_tid.get(index)?;
        if request.sent_at.elapsed() < self.request_timeout() {
            return Some(request);
        }
        None
    }

    /// Adds a [InflightRequest] with new transaction_id, and returns that id.
    pub fn add(&mut self, to: SocketAddrV4) -> u32 {
        let tid = self.get_next_tid();
        let request = InflightRequest {
            tid,
            to,
            sent_at: Instant::now(),
        };

        // Insert into requests_by_tid maintaining sort order
        let tid_index = self
            .requests_by_tid
            .binary_search_by(|r| r.tid.cmp(&tid))
            .unwrap_err();
        self.requests_by_tid.insert(tid_index, request.clone());

        // Insert into requests_by_time maintaining sort order
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
                // Since both vectors contain the same requests, this should always succeed
                if let Ok(time_index) = self
                    .requests_by_time
                    .binary_search_by(|r| r.sent_at.cmp(&request.sent_at))
                {
                    self.requests_by_time.remove(time_index);
                }

                self.update_rtt_estimates(request.sent_at.elapsed());

                Some(request)
            }
            Err(_) => None,
        }
    }

    fn update_rtt_estimates(&mut self, sample_rtt: Duration) {
        // Use TCP-like alpha = 1/8, beta = 1/4
        let alpha = 0.125;
        let beta = 0.25;

        let sample_rtt_secs = sample_rtt.as_secs_f64();
        let est_rtt_secs = self.estimated_rtt.as_secs_f64();
        let dev_rtt_secs = self.deviation_rtt.as_secs_f64();

        let new_est_rtt = (1.0 - alpha) * est_rtt_secs + alpha * sample_rtt_secs;
        let new_dev_rtt =
            (1.0 - beta) * dev_rtt_secs + beta * (sample_rtt_secs - new_est_rtt).abs();

        self.estimated_rtt = Duration::from_secs_f64(new_est_rtt);
        self.deviation_rtt = Duration::from_secs_f64(new_dev_rtt);
    }

    fn find_by_tid(&self, tid: u32) -> Result<usize, usize> {
        self.requests_by_tid
            .binary_search_by(|request| request.tid.cmp(&tid))
    }

    /// Removes timed-out requests if necessary to save memory
    pub fn cleanup(&mut self) {
        // Micro optimization to skip cleanup when vector is less than 90% full
        if self.requests_by_tid.len() < self.requests_by_tid.capacity() * 90 / 100 {
            return;
        }

        let index = match self
            .requests_by_time
            .binary_search_by(|request| self.request_timeout().cmp(&request.sent_at.elapsed()))
        {
            Ok(index) => index,
            Err(index) => index,
        };

        // Remove timed out requests from both vectors
        let timed_out_requests: Vec<_> = self.requests_by_time.drain(0..index).collect();

        // Remove the same requests from requests_by_tid
        for request in timed_out_requests {
            if let Ok(index) = self
                .requests_by_tid
                .binary_search_by(|r| r.tid.cmp(&request.tid))
            {
                self.requests_by_tid.remove(index);
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
    fn demonstrate_wrap_around_problem() {
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

        println!("Transaction IDs in vector:");
        for (i, request) in inflight.requests_by_tid.iter().enumerate() {
            println!("  [{}]: {}", i, request.tid);
        }

        // Try to find transaction ID 0 using binary search
        let search_tid = 0;
        let result = inflight.find_by_tid(search_tid);

        println!("Binary search for tid={}: {:?}", search_tid, result);

        // The problem: binary search expects a sorted vector, but our vector has:
        // [u32::MAX-2, u32::MAX-1, u32::MAX, 0, 1, 2]
        // This is NOT sorted by transaction ID due to wrap-around!

        // Binary search will fail or return incorrect results
        match result {
            Ok(index) => {
                println!("Binary search found tid={} at index {}", search_tid, index);
                println!("But the vector is not sorted by transaction ID due to wrap-around!");
                println!("This demonstrates why we need the double vector approach.");
            }
            Err(insert_index) => {
                println!("Binary search failed to find tid={}", search_tid);
                println!("Would insert at index {}", insert_index);
                println!("This demonstrates why we need the double vector approach.");
            }
        }
    }
}
