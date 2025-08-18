use std::collections::BTreeMap;
use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct InflightRequest {
    pub transaction_id: u32,
    pub to: SocketAddrV4,
    pub sent_at: Instant,
}

impl InflightRequest {
    pub fn does_match(&self, socket: &SocketAddrV4, tid: u32) -> bool {
        if self.transaction_id != tid {
            return false;
        }

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
    // BTreeMap provides O(log n) lookup, insertion, and deletion by transaction_id
    // while maintaining sorted order which enables efficient range operations
    // and deterministic iteration order.
    requests: BTreeMap<u32, InflightRequest>,
}

impl InflightRequests {
    pub fn new() -> Self {
        Self {
            requests: BTreeMap::new(),
        }
    }

    /// Add a new inflight request O(log n)
    pub fn add(&mut self, transaction_id: u32, to: SocketAddrV4) {
        self.requests.insert(
            transaction_id,
            InflightRequest {
                transaction_id,
                to,
                sent_at: Instant::now(),
            },
        );
    }

    /// Check if a transaction_id is still inflight O(log n)
    pub fn contains(&self, transaction_id: u32) -> bool {
        self.requests.contains_key(&transaction_id)
    }

    /// Remove inflight request by transaction_id if it exists and matches the address
    /// O(log n)
    pub fn remove(&mut self, transaction_id: u32, from: &SocketAddrV4) -> Option<InflightRequest> {
        let request = self.requests.get(&transaction_id)?;

        if !request.does_match(from, transaction_id) {
            // Early return if the source address doesn't match
            return None;
        }

        self.requests.remove(&transaction_id)
    }

    /// Cleanup expired requests based on timeout
    /// O(n) scans all requests to remove expired ones
    pub fn cleanup(&mut self, timeout: Duration) {
        let now = Instant::now();
        let cutoff = now - timeout;

        // Remove expired requests in a single pass using retain
        self.requests.retain(|_, request| request.sent_at > cutoff);
    }
}
