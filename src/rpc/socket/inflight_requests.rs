use std::collections::HashMap;
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
        let Some(&pos) = self.index.get(&transaction_id) else {
            // Early return if transaction_id is not in the index
            return None;
        };

        // This check ensures that even if the index contains a stale position, we don't panic.
        // This can happen if cleanup hasn't run yet after many removals.
        if pos >= self.requests.len() || self.requests[pos].transaction_id != transaction_id {
            return None;
        }

        let request = &self.requests[pos];
        if !request.does_match(from, transaction_id) {
            // Early return if the source address doesn't match
            return None;
        }

        self.index.remove(&transaction_id);

        // Mark as removed by swapping with a tombstone entry.
        // This is an efficient way to remove without resizing the Vec immediately.
        let mut removed = InflightRequest {
            transaction_id: 0, // Tombstone value
            to: request.to,
            sent_at: request.sent_at,
        };
        std::mem::swap(&mut removed, &mut self.requests[pos]);
        removed.transaction_id = transaction_id; // Restore original id for the return value

        Some(removed)
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
}
