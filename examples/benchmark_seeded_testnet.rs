use std::time::Instant;

fn main() {
    let start = Instant::now();
    mainline::Testnet::new(100).unwrap();
    println!("{:?}", start.elapsed());
}
