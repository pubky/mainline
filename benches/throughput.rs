use mainline::Testnet;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Measures how many concurrent GET queries a 100-node testnet can handle.
///
/// Fires 10, 50, and 100 parallel gets and reports wall-clock time,
/// queries-per-second, and per-query latency percentiles.
/// Catches contention issues, lock overhead, or degraded performance under load.
///
/// Run: `cargo run --release --features full --bin throughput`
fn main() {
    println!("throughput\n");

    for concurrency in [10, 50, 100] {
        bench_concurrent_gets(concurrency);
    }
}

fn bench_concurrent_gets(concurrency: usize) {
    let testnet = Testnet::new(100).unwrap();
    let nodes = Arc::new(testnet.nodes);

    // Store values
    let mut targets = Vec::with_capacity(concurrency);
    for i in 0..concurrency {
        let value = format!("throughput_{i}");
        let target = nodes[0].put_immutable(value.as_bytes()).unwrap();
        targets.push(target);
    }

    std::thread::sleep(Duration::from_millis(500));

    let start = Instant::now();

    let handles: Vec<_> = targets
        .iter()
        .enumerate()
        .map(|(i, target)| {
            let target = *target;
            let nodes = Arc::clone(&nodes);
            let node_idx = (i % 99) + 1;

            thread::spawn(move || {
                let t = Instant::now();
                let ok = nodes[node_idx].get_immutable(target).is_some();
                (t.elapsed(), ok)
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    let wall = start.elapsed();
    let ok = results.iter().filter(|(_, ok)| *ok).count();
    let qps = concurrency as f64 / wall.as_secs_f64();

    let mut latencies: Vec<_> = results.iter().map(|(d, _)| d.as_micros()).collect();
    latencies.sort_unstable();
    let n = latencies.len();
    let mean = latencies.iter().sum::<u128>() / n as u128;

    println!(
        "{concurrency} concurrent gets ({ok}/{concurrency} ok, {qps:.0} q/s, {:.2}s wall)",
        wall.as_secs_f64()
    );
    println!(
        "min={:.2}ms mean={:.2}ms p50={:.2}ms p95={:.2}ms max={:.2}ms\n",
        latencies[0] as f64 / 1000.0,
        mean as f64 / 1000.0,
        latencies[n / 2] as f64 / 1000.0,
        latencies[n * 95 / 100] as f64 / 1000.0,
        latencies[n - 1] as f64 / 1000.0,
    );
}
