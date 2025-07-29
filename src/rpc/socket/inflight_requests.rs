// This implementation has a critical flaw with high u32 transaction IDs.
//
// When transaction IDs wrap around (from u32::MAX to 0), the binary search
// in `find_by_tid()` fails because the vector is no longer sorted by transaction ID.
//
// Example: If we have requests with TIDs [u32::MAX-2, u32::MAX-1, u32::MAX, 0, 1, 2],
// the binary search will fail because 0 comes after u32::MAX in the vector but is
// numerically smaller.

use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct InflightRequest {
    pub(crate) tid: u32,
    pub(crate) to: SocketAddrV4,
    pub(crate) sent_at: Instant,
}

#[derive(Debug)]
/// We don't need a map, since we know the maximum size is `65535` requests.
/// Requests are also ordered by their transaction_id and thus sent_at, so lookup is fast.
pub struct InflightRequests {
    pub(crate) next_tid: u32,
    pub(crate) requests: Vec<InflightRequest>,
    estimated_rtt: Duration,
    deviation_rtt: Duration,
}

impl InflightRequests {
    pub fn new() -> Self {
        Self {
            next_tid: 0,
            requests: Vec::new(),
            estimated_rtt: Duration::from_secs(5),
            deviation_rtt: Duration::from_secs(0),
        }
    }

    /// Increments self.next_tid and returns the previous value.
    pub fn get_next_tid(&mut self) -> u32 {
        // Ordering will hold until wrap occurs at 4294967295 requests.
        // After wrap the ordering can break, but we can revisit if we ever hit that point.
        let tid = self.next_tid;
        self.next_tid = self.next_tid.wrapping_add(1);
        tid
    }

    pub fn request_timeout(&self) -> Duration {
        self.estimated_rtt + self.deviation_rtt.mul_f64(4.0)
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub fn estimated_rtt(&self) -> Duration {
        self.estimated_rtt
    }

    pub fn get(&self, key: u32) -> Option<&InflightRequest> {
        let index = self.find_by_tid(key).ok()?;
        let request = self.requests.get(index)?;
        if request.sent_at.elapsed() < self.request_timeout() {
            return Some(request);
        }
        None
    }

    /// Adds a [InflightRequest] with new transaction_id, and returns that id.
    pub fn add(&mut self, to: SocketAddrV4) -> u32 {
        let tid = self.get_next_tid();
        self.requests.push(InflightRequest {
            tid,
            to,
            sent_at: Instant::now(),
        });

        tid
    }

    pub fn remove(&mut self, key: u32) -> Option<InflightRequest> {
        match self.find_by_tid(key) {
            Ok(index) => {
                let request = self.requests.remove(index);

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
        self.requests
            .binary_search_by(|request| request.tid.cmp(&tid))
    }

    /// Removes timed-out requests if necessary to save memory
    pub fn cleanup(&mut self) {
        // Micro optimization to skip cleanup when vector is less than 90% full
        if self.requests.len() < self.requests.capacity() * 90 / 100 {
            return;
        }

        let index = match self
            .requests
            .binary_search_by(|request| self.request_timeout().cmp(&request.sent_at.elapsed()))
        {
            Ok(index) => index,
            Err(index) => index,
        };

        self.requests.drain(0..index);
    }
}
