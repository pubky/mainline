use std::collections::BTreeMap;
use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

const MIN_TIMEOUT_MS: u64 = 500;
const MAX_TIMEOUT_MS: u64 = 10000;
const INITIAL_ESTIMATED_RTT_MS: u64 = 2000;
const DEVIATION_RTT_MS: u64 = 500;
/// Conservative learning rate for estimated RTT (lower = more stable, higher = faster adaptation)
const ALPHA: f64 = 0.05;
/// Conservative learning rate for RTT deviation (lower = more stable, higher = faster adaptation)
const BETA: f64 = 0.1;

#[derive(Debug, Clone)]
pub struct InflightRequest {
    pub to: SocketAddrV4,
    pub sent_at: Instant,
}

impl InflightRequest {
    pub fn does_match(&self, socket: &SocketAddrV4) -> bool {
        if self.to.port() != socket.port() {
            return false;
        }

        if self.to.ip().is_unspecified() {
            return true;
        }

        self.to.ip() == socket.ip()
    }
}

#[derive(Debug)]
pub struct InflightRequests {
    // BTreeMap provides O(log n) lookup, insertion, and deletion keyed by transaction_id.
    requests: BTreeMap<u32, InflightRequest>,
    estimated_rtt: Duration,
    deviation_rtt: Duration,
}

impl InflightRequests {
    pub fn new() -> Self {
        Self {
            requests: BTreeMap::new(),
            estimated_rtt: Duration::from_millis(INITIAL_ESTIMATED_RTT_MS),
            deviation_rtt: Duration::from_millis(DEVIATION_RTT_MS),
        }
    }

    /// Add a new inflight request O(log n)
    pub fn add(&mut self, transaction_id: u32, to: SocketAddrV4) {
        self.requests.insert(
            transaction_id,
            InflightRequest {
                to,
                sent_at: Instant::now(),
            },
        );
    }

    /// Check if a transaction_id is still inflight and not expired O(log n)
    pub fn contains(&self, transaction_id: u32) -> bool {
        if let Some(request) = self.requests.get(&transaction_id) {
            return request.sent_at.elapsed() < self.request_timeout();
        }
        false
    }

    /// Remove inflight request by transaction_id if it exists and matches the address
    /// O(log n)
    pub fn remove(&mut self, transaction_id: u32, from: &SocketAddrV4) -> Option<InflightRequest> {
        let request = self.requests.get(&transaction_id)?;

        let elapsed = request.sent_at.elapsed();
        if elapsed >= self.request_timeout() {
            self.requests.remove(&transaction_id);
            return None;
        }

        if !request.does_match(from) {
            return None;
        }

        let request = self.requests.remove(&transaction_id)?;

        self.update_rtt_estimates(elapsed);

        Some(request)
    }

    /// Check if there are no inflight requests
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    fn request_timeout(&self) -> Duration {
        let timeout = self.estimated_rtt + self.deviation_rtt.mul_f64(4.0);
        timeout
            .max(Duration::from_millis(MIN_TIMEOUT_MS))
            .min(Duration::from_millis(MAX_TIMEOUT_MS))
    }

    /// Updates RTT estimates using exponentially weighted moving averages (EWMA).
    /// - Estimated RTT = (1-α) * old_estimate + α * sample
    /// - Deviation RTT = (1-β) * old_deviation + β * |sample - new_estimate|
    ///
    /// Conservative learning rates (α=0.05, β=0.1) make the algorithm less sensitive to
    /// temporary network fluctuations for stable DHT timeout calculations.
    fn update_rtt_estimates(&mut self, sample_rtt: Duration) {
        let sample_rtt_secs = sample_rtt.as_secs_f64();
        let est_rtt_secs = self.estimated_rtt.as_secs_f64();
        let dev_rtt_secs = self.deviation_rtt.as_secs_f64();

        // Update estimated RTT using exponentially weighted moving average
        let new_est_rtt = (1.0 - ALPHA) * est_rtt_secs + ALPHA * sample_rtt_secs;

        // Update deviation RTT based on absolute difference from new estimate
        let new_dev_rtt =
            (1.0 - BETA) * dev_rtt_secs + BETA * (sample_rtt_secs - new_est_rtt).abs();

        self.estimated_rtt = Duration::from_secs_f64(new_est_rtt);
        self.deviation_rtt = Duration::from_secs_f64(new_dev_rtt);
    }

    /// Cleanup expired requests based on adaptive timeout
    /// O(n) scans all requests to remove expired ones
    pub fn cleanup(&mut self) {
        let timeout = self.request_timeout();
        let now = Instant::now();
        let cutoff = now - timeout;

        // Remove expired requests in a single pass using retain
        self.requests.retain(|_, request| request.sent_at > cutoff);
    }
}
