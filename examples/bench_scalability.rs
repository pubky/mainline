//! Query cost at varying network sizes.
//!
//! Runs the same workload across testnets of 10 to 100 nodes and reports
//! init time and get latency (p50/p95). In a Kademlia DHT, query cost
//! should grow O(log n) — if these numbers grow faster, there is a
//! scaling bug in the tick loop or routing table maintenance.
//!
//! Run: `cargo run --release --example bench_scalability`

use mainline::Testnet;
use std::time::Instant;

fn main() {
    println!("scalability\n");
    println!(
        "{:<8} {:<10} {:<10} {:<10}",
        "nodes", "init", "get_p50", "get_p95"
    );

    for size in [10, 25, 50, 100] {
        let init_start = Instant::now();
        let testnet = Testnet::builder(size).build().unwrap();
        let init = init_start.elapsed();
        let nodes = &testnet.nodes;

        // Publish a value, then measure gets from different nodes.
        let target = nodes[0].put_immutable(b"bench_scaling").unwrap();

        let samples = 20;
        let mut ms: Vec<f64> = Vec::with_capacity(samples);

        for i in 0..samples {
            let node_idx = if size > 1 { (i % (size - 1)) + 1 } else { 0 };
            let start = Instant::now();
            let _ = nodes[node_idx].get_immutable(target);
            ms.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = ms.len();

        println!(
            "{:<8} {:<10} {:<10} {:<10}",
            size,
            format!("{:.2}s", init.as_secs_f64()),
            format!("{:.1}ms", ms[n / 2]),
            format!("{:.1}ms", ms[n * 95 / 100]),
        );
    }
}
