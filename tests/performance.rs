//! Performance and reliability test for mainline DHT.
//!
//! Tests mainline's core DHT operations and measures key performance/reliability metrics.
//!
//! Run with: cargo test performance

use mainline::{Dht, DhtBuilder, Id, MutableItem, Node};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[derive(Debug, Default)]
struct OperationMetrics {
    operations: AtomicU64,
    successes: AtomicU64,
    failures: AtomicU64,
    timeouts: AtomicU64,
    total_latency_ms: AtomicU64,
    max_latency_ms: AtomicU64,
}

#[derive(Debug, Default)]
struct TestMetrics {
    find_node: OperationMetrics,
    put_immutable: OperationMetrics,
    get_immutable: OperationMetrics,
    put_mutable: OperationMetrics,
    get_mutable: OperationMetrics,
    get_peers: OperationMetrics,
}

impl OperationMetrics {
    fn record_success(&self, latency_ms: u64) {
        self.operations.fetch_add(1, Ordering::Relaxed);
        self.successes.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);

        // Track maximum latency using atomic compare-and-swap to handle concurrent updates
        loop {
            let current_max = self.max_latency_ms.load(Ordering::Relaxed);
            if latency_ms <= current_max
                || self
                    .max_latency_ms
                    .compare_exchange_weak(
                        current_max,
                        latency_ms,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok()
            {
                break;
            }
        }
    }

    fn record_failure(&self) {
        self.operations.fetch_add(1, Ordering::Relaxed);
        self.failures.fetch_add(1, Ordering::Relaxed);
    }

    fn record_timeout(&self) {
        self.operations.fetch_add(1, Ordering::Relaxed);
        self.timeouts.fetch_add(1, Ordering::Relaxed);
    }

    fn print_summary(&self, operation_name: &str) {
        let ops = self.operations.load(Ordering::Relaxed);
        let successes = self.successes.load(Ordering::Relaxed);
        let failures = self.failures.load(Ordering::Relaxed);
        let timeouts = self.timeouts.load(Ordering::Relaxed);
        let total_latency = self.total_latency_ms.load(Ordering::Relaxed);
        let max_latency = self.max_latency_ms.load(Ordering::Relaxed);

        if ops > 0 {
            println!("\n--- {} ---", operation_name);
            println!("Operations: {} total", ops);
            println!(
                "Successes: {} ({:.1}%)",
                successes,
                (successes as f64 / ops as f64) * 100.0
            );
            println!(
                "Failures: {} ({:.1}%)",
                failures,
                (failures as f64 / ops as f64) * 100.0
            );
            println!(
                "Timeouts: {} ({:.1}%)",
                timeouts,
                (timeouts as f64 / ops as f64) * 100.0
            );
            println!(
                "Avg Latency: {:.2}ms",
                if successes > 0 {
                    total_latency as f64 / successes as f64
                } else {
                    0.0
                }
            );
            println!("Max Latency: {}ms", max_latency);
        }
    }
}

impl TestMetrics {
    fn new() -> Self {
        Self::default()
    }

    fn print_summary(&self) {
        println!("\n=== MAINLINE DHT PERFORMANCE TEST RESULTS ===");
        self.find_node.print_summary("find_node");
        self.put_immutable.print_summary("put_immutable");
        self.get_immutable.print_summary("get_immutable");
        self.put_mutable.print_summary("put_mutable");
        self.get_mutable.print_summary("get_mutable");
        self.get_peers.print_summary("get_peers");
    }
}

#[test]
fn test_dht_performance() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting mainline DHT performance test");

    // Create a DHT instance for testing with bootstrap nodes
    let dht = Dht::builder().build()?;

    let metrics = TestMetrics::new();
    let test_start = Instant::now();

    // Test basic find_node operations
    println!("Testing find_node operations...");
    for i in 0..10 {
        let target_id = Id::random();

        let start = Instant::now();
        let nodes = dht.find_node(target_id);
        let latency = start.elapsed();

        if !nodes.is_empty() {
            metrics.find_node.record_success(latency.as_millis() as u64);
        } else {
            metrics.find_node.record_failure();
        }
    }

    // Test immutable item operations (get_immutable/put_immutable)
    println!("Testing immutable item operations...");
    for i in 0..5 {
        let value = format!("test-value-{}", i).into_bytes();

        // Test put_immutable
        let start = Instant::now();
        let put_result = dht.put_immutable(&value);
        let put_latency = start.elapsed();

        match put_result {
            Ok(target_id) => {
                metrics
                    .put_immutable
                    .record_success(put_latency.as_millis() as u64);

                // Test get_immutable
                let start = Instant::now();
                let get_result = dht.get_immutable(target_id);
                let get_latency = start.elapsed();

                match get_result {
                    Some(_) => {
                        metrics
                            .get_immutable
                            .record_success(get_latency.as_millis() as u64);
                    }
                    None => {
                        metrics.get_immutable.record_failure();
                    }
                }
            }
            Err(_) => {
                metrics.put_immutable.record_failure();
            }
        }
    }

    // Test mutable item operations
    println!("Testing mutable item operations...");
    for i in 0..5 {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).expect("getrandom failed");
        let keypair = ed25519_dalek::SigningKey::from_bytes(&bytes);
        let value = format!("mutable-value-{}", i).into_bytes();
        let seq = 1000 + i as i64;

        let item = MutableItem::new(keypair.clone(), &value, seq, None);

        // Test put_mutable
        let start = Instant::now();
        let put_result = dht.put_mutable(item.clone(), None);
        let put_latency = start.elapsed();

        match put_result {
            Ok(_) => {
                metrics
                    .put_mutable
                    .record_success(put_latency.as_millis() as u64);

                // Test get_mutable
                let start = Instant::now();
                let get_result = dht
                    .get_mutable(keypair.verifying_key().as_bytes(), None, None)
                    .next();
                let get_latency = start.elapsed();

                match get_result {
                    Some(_) => {
                        metrics
                            .get_mutable
                            .record_success(get_latency.as_millis() as u64);
                    }
                    None => {
                        metrics.get_mutable.record_failure();
                    }
                }
            }
            Err(_) => {
                metrics.put_mutable.record_failure();
            }
        }
    }

    // Test get_peers operations
    println!("Testing get_peers operations...");
    for i in 0..5 {
        let info_hash = Id::random();

        let start = Instant::now();
        let peers = dht.get_peers(info_hash);
        let peer_count = peers.count();
        let latency = start.elapsed();

        if peer_count > 0 {
            metrics.get_peers.record_success(latency.as_millis() as u64);
        } else {
            metrics.get_peers.record_failure();
        }
    }

    let total_duration = test_start.elapsed();
    println!("Test completed in {:?}", total_duration);

    metrics.print_summary();

    Ok(())
}
