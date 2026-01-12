use std::time::Instant;

fn main() {
    let start = Instant::now();
    mainline::Testnet::new_unseeded(100).unwrap();
    println!("{:?}", start.elapsed());
}
