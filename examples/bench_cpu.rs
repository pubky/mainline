//! Measures CPU utilization of a DHT node under homeserver-like conditions.
//!
//! Emulates how pubky-homeserver uses mainline:
//!   - Server mode on the real DHT
//!   - Periodic put_mutable to republish signed packets (homeserver key + user keys)
//!   - get_mutable to verify publication reached enough nodes
//!
//! Reports CPU % for idle maintenance vs active republish cycles.
//!
//! Run: `cargo run --release --example bench_cpu`

use cpu_time::ProcessTime;
use mainline::{Dht, MutableItem, SigningKey};
use std::thread;
use std::time::{Duration, Instant};

fn random_signing_key() -> SigningKey {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("getrandom failed");
    SigningKey::from_bytes(&bytes)
}

/// Number of simulated user keys to republish.
const USER_COUNT: usize = 10;

/// Homeserver republishes user keys with up to 12 concurrent threads.
const REPUBLISH_CONCURRENCY: usize = 12;

fn main() {
    println!("cpu_usage (homeserver simulation, {USER_COUNT} users)\n");

    // Homeserver starts a server-mode DHT node on the real network.
    let dht = Dht::server().expect("failed to create DHT node");

    print!("bootstrapping...");
    let start = Instant::now();
    while !dht.bootstrapped() {
        if start.elapsed() > Duration::from_secs(30) {
            println!(" timeout after 30s");
            return;
        }
        thread::sleep(Duration::from_secs(1));
    }
    println!(" done ({:.1}s)\n", start.elapsed().as_secs_f64());

    // Phase 1: idle — node does routing table maintenance and responds to
    // incoming queries from the real network. This is what a homeserver
    // looks like between republish cycles (most of the time).
    let phase_duration = Duration::from_secs(30);

    let cpu_before = ProcessTime::now();
    let wall_before = Instant::now();
    thread::sleep(phase_duration);
    let idle_cpu = cpu_before.elapsed();
    let idle_wall = wall_before.elapsed();

    println!("idle ({:.0}s)", idle_wall.as_secs_f64());
    println!(
        "  cpu: {:.2}s / {:.1}s = {:.1}%\n",
        idle_cpu.as_secs_f64(),
        idle_wall.as_secs_f64(),
        cpu_pct(idle_cpu, idle_wall),
    );

    // Phase 2: republish cycle — simulate what the homeserver does every 4 hours.
    // Generate a homeserver key + N user keys, then publish them all with
    // put_mutable and verify with get_mutable, using 12 concurrent threads
    // (matching the homeserver's MultiRepublisher concurrency).
    let homeserver_key = random_signing_key();
    let user_keys: Vec<SigningKey> = (0..USER_COUNT)
        .map(|_| random_signing_key())
        .collect();

    // Collect all keys: homeserver + users.
    let mut all_keys = vec![&homeserver_key];
    all_keys.extend(user_keys.iter());

    let cpu_before = ProcessTime::now();
    let wall_before = Instant::now();

    // Republish in batches of REPUBLISH_CONCURRENCY, matching homeserver behavior.
    let mut published = 0usize;
    let mut verified = 0usize;

    for chunk in all_keys.chunks(REPUBLISH_CONCURRENCY) {
        let handles: Vec<_> = chunk
            .iter()
            .map(|key| {
                let dht = dht.clone();
                let item = MutableItem::new((*key).clone(), b"bench_cpu_packet", 1, None);
                let pub_key = key.verifying_key().to_bytes();

                thread::spawn(move || {
                    // put_mutable — publish the signed packet.
                    let put_ok = dht.put_mutable(item, None).is_ok();

                    // get_mutable — verify it reached nodes (homeserver counts responses).
                    let mut node_count = 0usize;
                    if put_ok {
                        for _item in dht.get_mutable(&pub_key, None, None) {
                            node_count += 1;
                        }
                    }

                    (put_ok, node_count)
                })
            })
            .collect();

        for h in handles {
            if let Ok((put_ok, node_count)) = h.join() {
                if put_ok {
                    published += 1;
                }
                if node_count > 0 {
                    verified += 1;
                }
            }
        }
    }

    let republish_cpu = cpu_before.elapsed();
    let republish_wall = wall_before.elapsed();

    println!(
        "republish cycle ({} keys, {} published, {} verified, {:.0}s)",
        all_keys.len(),
        published,
        verified,
        republish_wall.as_secs_f64(),
    );
    println!(
        "  cpu: {:.2}s / {:.1}s = {:.1}%\n",
        republish_cpu.as_secs_f64(),
        republish_wall.as_secs_f64(),
        cpu_pct(republish_cpu, republish_wall),
    );
}

fn cpu_pct(cpu: Duration, wall: Duration) -> f64 {
    cpu.as_secs_f64() / wall.as_secs_f64() * 100.0
}
