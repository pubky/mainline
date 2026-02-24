//! Single-operation latency on a 100-node local testnet.
//!
//! Measures wall-clock time for individual get_immutable and put_immutable
//! calls. Catches regressions in the iterative query algorithm, routing
//! table lookups, or actor tick processing.
//!
//! Run: `cargo run --release --example bench_latency`

use mainline::Testnet;
use std::time::{Duration, Instant};

fn main() {
    println!("latency\n");

    let size = 100;
    let testnet = Testnet::builder(size).build().unwrap();
    let nodes = &testnet.nodes;

    // GET: publish a value, then time gets from different nodes.
    let target = nodes[0].put_immutable(b"bench_latency_get").unwrap();

    let samples = 30;
    let mut timings = Vec::with_capacity(samples);

    for i in 0..samples {
        let node_idx = (i % (size - 1)) + 1;
        let start = Instant::now();
        let _ = nodes[node_idx].get_immutable(target);
        timings.push(start.elapsed());
    }

    println!("get_immutable ({size} nodes, {samples} samples)");
    print_stats(&timings);

    // PUT: time each put_immutable individually.
    let samples = 10;
    let mut timings = Vec::with_capacity(samples);

    for i in 0..samples {
        let value = format!("bench_latency_put_{i}");
        let node_idx = (i % (size - 1)) + 1;
        let start = Instant::now();
        let _ = nodes[node_idx].put_immutable(value.as_bytes());
        timings.push(start.elapsed());
    }

    println!("put_immutable ({size} nodes, {samples} samples)");
    print_stats(&timings);
}

fn print_stats(timings: &[Duration]) {
    let mut ms: Vec<f64> = timings.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = ms.len();
    let mean: f64 = ms.iter().sum::<f64>() / n as f64;

    println!(
        "  min={:.1}ms  mean={:.1}ms  p50={:.1}ms  p95={:.1}ms  max={:.1}ms\n",
        ms[0],
        mean,
        ms[n / 2],
        ms[n * 95 / 100],
        ms[n - 1],
    );
}
