use mainline::Testnet;
use std::time::{Duration, Instant};

/// End-to-end latency for get/put operations on a local testnet.
///
/// Note: latency is bounded below by the actor tick interval (READ_TIMEOUT = 50ms),
/// so these numbers reflect query round-trips in ticks, not raw CPU cost.
/// The useful signal is relative differences between runs, not absolute values.
fn main() {
    println!("latency\n");

    for size in [10, 100] {
        let testnet = Testnet::new(size).unwrap();
        let nodes = &testnet.nodes;

        // Seed a value from node 0
        let target = nodes[0].put_immutable(b"bench_payload").unwrap();

        // Let it propagate
        std::thread::sleep(Duration::from_millis(200));

        // GET
        let samples = 30;
        let mut timings = Vec::with_capacity(samples);
        let mut misses = 0;

        for i in 0..samples {
            let node_idx = (i % (size - 1)) + 1;
            let start = Instant::now();
            let result = nodes[node_idx].get_immutable(target);
            timings.push(start.elapsed());
            if result.is_none() {
                misses += 1;
            }
        }

        println!("get_immutable ({size} nodes, {misses} misses)");
        print_stats(&timings);

        // PUT
        let samples = 20;
        let mut timings = Vec::with_capacity(samples);

        for i in 0..samples {
            let value = format!("put_bench_{size}_{i}");
            let node_idx = i % size;
            let start = Instant::now();
            let _ = nodes[node_idx].put_immutable(value.as_bytes());
            timings.push(start.elapsed());
        }

        println!("put_immutable ({size} nodes)");
        print_stats(&timings);
    }
}

fn print_stats(timings: &[Duration]) {
    let mut us: Vec<_> = timings.iter().map(|d| d.as_micros()).collect();
    us.sort_unstable();
    let n = us.len();
    let mean = us.iter().sum::<u128>() / n as u128;

    println!(
        "n={n} min={:.2}ms mean={:.2}ms p50={:.2}ms p95={:.2}ms max={:.2}ms\n",
        us[0] as f64 / 1000.0,
        mean as f64 / 1000.0,
        us[n / 2] as f64 / 1000.0,
        us[n * 95 / 100] as f64 / 1000.0,
        us[n - 1] as f64 / 1000.0,
    );
}
