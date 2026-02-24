//! Concurrent query throughput on a 100-node local testnet.
//!
//! Fires N parallel get_immutable calls and measures total wall-clock time
//! and ops/sec. Increasing concurrency should scale roughly linearly —
//! if ops/sec drops or wall time grows disproportionately, there is
//! contention in the actor loop or channel backpressure.
//!
//! Each concurrency level is run 5 times; the median is reported to
//! reduce noise from CPU scheduling and localhost contention.
//!
//! Run: `cargo run --release --example bench_throughput`

use mainline::Testnet;
use std::thread;
use std::time::Instant;

const RUNS: usize = 5;

fn main() {
    println!("throughput ({RUNS} runs, median)\n");

    let testnet = Testnet::builder(100).build().unwrap();

    // Publish values upfront so all gets have something to find.
    let max_concurrency = 100;
    let mut targets = Vec::with_capacity(max_concurrency);
    for i in 0..max_concurrency {
        let value = format!("bench_throughput_{i}");
        let target = testnet.nodes[0].put_immutable(value.as_bytes()).unwrap();
        targets.push(target);
    }

    println!(
        "{:<12} {:<10} {:<10} {:<10}",
        "concurrency", "wall", "ops/s", "ok"
    );

    for concurrency in [1, 10, 50, 100] {
        let mut ops_per_sec = Vec::with_capacity(RUNS);
        let mut walls = Vec::with_capacity(RUNS);
        let mut all_ok = true;

        for _ in 0..RUNS {
            let start = Instant::now();

            let handles: Vec<_> = targets[..concurrency]
                .iter()
                .enumerate()
                .map(|(i, &target)| {
                    let dht = testnet.nodes[(i % 99) + 1].clone();
                    thread::spawn(move || dht.get_immutable(target).is_some())
                })
                .collect();

            let ok = handles
                .into_iter()
                .filter_map(|h| h.join().ok())
                .filter(|&ok| ok)
                .count();

            let wall = start.elapsed();
            ops_per_sec.push(concurrency as f64 / wall.as_secs_f64());
            walls.push(wall.as_secs_f64());

            if ok != concurrency {
                all_ok = false;
            }
        }

        ops_per_sec.sort_by(|a, b| a.partial_cmp(b).unwrap());
        walls.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_ops = ops_per_sec[RUNS / 2];
        let median_wall = walls[RUNS / 2];

        println!(
            "{:<12} {:<10} {:<10} {}",
            concurrency,
            format!("{:.2}s", median_wall),
            format!("{:.0}", median_ops),
            if all_ok { "ok" } else { "MISS" },
        );
    }
}
