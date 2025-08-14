use mainline::{Dht, Id, MutableItem, SigningKey};
use std::time::{Duration, Instant};
use tracing::Level;

const NUM_ITERATIONS: usize = 5;
const WARMUP_ITERATIONS: usize = 2;

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("=== DHT Operations Benchmark ===\n");

    let dht = Dht::client().expect("Failed to create DHT client");

    // Warmup
    println!("Warming up...");
    for _ in 0..WARMUP_ITERATIONS {
        let _ = dht.info();
    }

    benchmark_find_node(&dht);
    benchmark_immutable_operations(&dht);
    benchmark_mutable_operations(&dht);
    benchmark_peer_operations(&dht);
    benchmark_info_operations(&dht);

    println!("\n=== Benchmark Complete ===");
}

fn benchmark_find_node(dht: &Dht) {
    println!("🔍 Benchmarking find_node operations:");

    let mut times = Vec::new();

    for i in 0..NUM_ITERATIONS {
        let random_id = Id::random();
        let start = Instant::now();

        let _result = dht.find_node(random_id);

        let elapsed = start.elapsed();
        times.push(elapsed);

        println!("  Run {}: {:?}", i + 1, elapsed);
    }

    print_stats("find_node", &times);
}

fn benchmark_immutable_operations(dht: &Dht) {
    println!("\n📦 Benchmarking immutable operations:");

    let test_data = b"benchmark test data for immutable storage";
    let mut put_times = Vec::new();
    let mut get_times = Vec::new();

    // Benchmark put_immutable
    println!("  put_immutable:");
    for i in 0..NUM_ITERATIONS {
        let start = Instant::now();
        let _info_hash = dht.put_immutable(test_data);
        let elapsed = start.elapsed();
        put_times.push(elapsed);
        println!("    Run {}: {:?}", i + 1, elapsed);
    }

    // Store one for get testing
    let info_hash = dht
        .put_immutable(test_data)
        .expect("Failed to store test data");

    // Benchmark get_immutable
    println!("  get_immutable:");
    for i in 0..NUM_ITERATIONS {
        let start = Instant::now();
        let _result = dht.get_immutable(info_hash);
        let elapsed = start.elapsed();
        get_times.push(elapsed);
        println!("    Run {}: {:?}", i + 1, elapsed);
    }

    print_stats("put_immutable", &put_times);
    print_stats("get_immutable", &get_times);
}

fn benchmark_mutable_operations(dht: &Dht) {
    println!("\n🔐 Benchmarking mutable operations:");

    let signing_key = test_signing_key();
    let public_key = signing_key.verifying_key().to_bytes();
    let test_data = b"benchmark test data for mutable storage";

    let mut put_times = Vec::new();
    let mut get_times = Vec::new();

    // Benchmark put_mutable
    println!("  put_mutable:");
    for i in 0..NUM_ITERATIONS {
        let item = MutableItem::new(signing_key.clone(), test_data, i as i64 + 1, None);
        let start = Instant::now();
        let _result = dht.put_mutable(item, None);
        let elapsed = start.elapsed();
        put_times.push(elapsed);
        println!("    Run {}: {:?}", i + 1, elapsed);
    }

    // Benchmark get_mutable
    println!("  get_mutable:");
    for i in 0..NUM_ITERATIONS {
        let start = Instant::now();
        let _result = dht.get_mutable(&public_key, None, None);
        let elapsed = start.elapsed();
        get_times.push(elapsed);
        println!("    Run {}: {:?}", i + 1, elapsed);
    }

    print_stats("put_mutable", &put_times);
    print_stats("get_mutable", &get_times);
}

fn benchmark_peer_operations(dht: &Dht) {
    println!("\n👥 Benchmarking peer operations:");

    let test_info_hash = Id::random();
    let test_port = 8080u16;

    let mut announce_times = Vec::new();
    let mut get_peers_times = Vec::new();

    // Benchmark announce_peer
    println!("  announce_peer:");
    for i in 0..NUM_ITERATIONS {
        let start = Instant::now();
        let _result = dht.announce_peer(test_info_hash, Some(test_port));
        let elapsed = start.elapsed();
        announce_times.push(elapsed);
        println!("    Run {}: {:?}", i + 1, elapsed);
    }

    // Benchmark get_peers
    println!("  get_peers:");
    for i in 0..NUM_ITERATIONS {
        let start = Instant::now();
        let _result = dht.get_peers(test_info_hash);
        let elapsed = start.elapsed();
        get_peers_times.push(elapsed);
        println!("    Run {}: {:?}", i + 1, elapsed);
    }

    print_stats("announce_peer", &announce_times);
    print_stats("get_peers", &get_peers_times);
}

fn benchmark_info_operations(dht: &Dht) {
    println!("\n📊 Benchmarking info operations:");

    let mut info_times = Vec::new();
    let mut estimate_times = Vec::new();

    // Benchmark dht.info()
    println!("  dht_info:");
    for i in 0..NUM_ITERATIONS {
        let start = Instant::now();
        let _info = dht.info();
        let elapsed = start.elapsed();
        info_times.push(elapsed);
        println!("    Run {}: {:?}", i + 1, elapsed);
    }

    // Benchmark dht_size_estimate
    println!("  dht_size_estimate:");
    for i in 0..NUM_ITERATIONS {
        let info = dht.info();
        let start = Instant::now();
        let _estimate = info.dht_size_estimate();
        let elapsed = start.elapsed();
        estimate_times.push(elapsed);
        println!("    Run {}: {:?}", i + 1, elapsed);
    }

    print_stats("dht_info", &info_times);
    print_stats("dht_size_estimate", &estimate_times);
}

fn test_signing_key() -> SigningKey {
    // Fixed test key for reproducible benchmarks
    let bytes = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];
    SigningKey::from_bytes(&bytes)
}

fn print_stats(operation: &str, times: &[Duration]) {
    let total: Duration = times.iter().sum();
    let mean = total / times.len() as u32;

    let variance: f64 = times
        .iter()
        .map(|time| {
            let diff = time.as_nanos() as f64 - mean.as_nanos() as f64;
            diff * diff
        })
        .sum::<f64>()
        / times.len() as f64;

    let std_dev = Duration::from_nanos(variance.sqrt() as u64);

    let min = *times.iter().min().unwrap();
    let max = *times.iter().max().unwrap();

    println!("  📈 {} stats:", operation);
    println!("    Mean: {:?}", mean);
    println!("    Std Dev: {:?}", std_dev);
    println!("    Min: {:?}", min);
    println!("    Max: {:?}", max);
}
