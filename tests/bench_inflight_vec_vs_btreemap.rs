use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const SIZES: [usize; 3] = [500, 5000, 20000];
const ITERS: usize = 50_000;

#[derive(Clone)]
struct Inflight {
    sent_at: Instant,
}

fn now() -> Instant {
    Instant::now()
}

#[test]
fn bench_inflight_vec_vs_btreemap() {
    for &size in &SIZES {
        println!("\n-- Inflight size: {} --", size);
        let mut tids: Vec<u16> = (0..u16::MAX).take(size).collect();
        let target = tids[size / 2];

        // Vec with binary search
        let mut vec: Vec<(u16, Inflight)> = tids
            .iter()
            .map(|&k| (k, Inflight { sent_at: now() }))
            .collect();

        let start = Instant::now();
        for i in 0..ITERS {
            // Insert at front
            vec.insert(0, (9999, Inflight { sent_at: now() }));

            // Lookup
            let _ = vec.binary_search_by_key(&target, |(k, _)| *k);

            // Cleanup expired (simulate 10% chance)
            if i % 10 == 0 {
                let cutoff = now() - Duration::from_secs(1);
                vec.retain(|(_, inflight)| inflight.sent_at > cutoff);
            }
        }
        println!("Vec (binary search + retain): {:?}", start.elapsed());

        // BTreeMap
        let mut map: BTreeMap<u16, Inflight> = tids
            .iter()
            .map(|&k| (k, Inflight { sent_at: now() }))
            .collect();

        let start = Instant::now();
        for i in 0..ITERS {
            map.insert(9999, Inflight { sent_at: now() });

            let _ = map.get(&target);

            if i % 10 == 0 {
                let cutoff = now() - Duration::from_secs(1);
                map.retain(|_, inflight| inflight.sent_at > cutoff);
            }
        }
        println!("BTreeMap (lookup + retain): {:?}", start.elapsed());
    }
}
