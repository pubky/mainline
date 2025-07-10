use std::collections::BTreeMap;
use std::time::Instant;

const SIZES: [usize; 3] = [500, 5000, 20000];
const ITERS: usize = 100_000;

#[test]
fn bench_inflight_vec_vs_btreemap() {
    for &size in &SIZES {
        println!("\n-- Inflight size: {} --", size);
        let keys: Vec<u16> = (0..size as u16).collect();
        let target = keys[size / 2];

        // Vec
        let mut vec: Vec<(u16, u8)> = keys.iter().map(|&k| (k, 1)).collect();
        let start = Instant::now();
        for _ in 0..ITERS {
            let _ = vec.iter().find(|(k, _)| *k == target);
        }
        println!("Vec lookup:   {:?}", start.elapsed());

        let start = Instant::now();
        vec.push((9999, 1)); // insert
        let _ = vec.iter().find(|(k, _)| *k == 9999); // lookup
        vec.retain(|(k, _)| *k != 9999); // cleanup
        println!("Vec insert+lookup+retain: {:?}", start.elapsed());

        // BTreeMap
        let mut map: BTreeMap<u16, u8> = keys.iter().map(|&k| (k, 1)).collect();
        let start = Instant::now();
        for _ in 0..ITERS {
            let _ = map.get(&target);
        }
        println!("BTreeMap lookup: {:?}", start.elapsed());

        let start = Instant::now();
        map.insert(9999, 1); // insert
        let _ = map.get(&9999); // lookup
        map.remove(&9999); // cleanup
        println!("BTreeMap insert+lookup+remove: {:?}", start.elapsed());
    }
}
