use mainline::Testnet;
use std::time::{Duration, Instant};

/// Measures testnet init time and GET latency (p50/p95) across increasing
/// network sizes (5 to 100 nodes).
///
/// Catches super-linear scaling in init or query cost — e.g. O(n^2) routing
/// table operations. Compare columns between runs to spot regressions.
///
/// Run: `cargo run --release --features full --bin scalability`
fn main() {
    println!("scalability\n");
    println!(
        "{:<7} {:<12} {:<12} {:<12}",
        "nodes", "init", "get_p50", "get_p95"
    );

    for size in [5, 10, 25, 50, 100] {
        let init_start = Instant::now();
        let testnet = Testnet::new(size).unwrap();
        let init = init_start.elapsed();

        let nodes = &testnet.nodes;
        std::thread::sleep(Duration::from_millis(100));

        let target = nodes[0].put_immutable(b"scale_test").unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let samples = 20;
        let mut us: Vec<u128> = Vec::with_capacity(samples);

        for i in 0..samples {
            let node_idx = if size > 1 { (i % (size - 1)) + 1 } else { 0 };
            let start = Instant::now();
            let _ = nodes[node_idx].get_immutable(target);
            us.push(start.elapsed().as_micros());
        }
        us.sort_unstable();
        let n = us.len();

        println!(
            "{:<7} {:<12} {:<12} {:<12}",
            size,
            format!("{:.2}s", init.as_secs_f64()),
            format!("{:.2}ms", us[n / 2] as f64 / 1000.0),
            format!("{:.2}ms", us[n * 95 / 100] as f64 / 1000.0),
        );
    }
}
