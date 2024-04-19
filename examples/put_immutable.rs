use std::time::Instant;

use mainline::{Bytes, Dht};

use clap::Parser;

use tracing::Level;
use tracing_subscriber;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Value to store on the DHT
    value: String,
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();

    let dht = Dht::client().unwrap();
    let value = Bytes::from(cli.value.to_owned());

    println!("\nStoring immutable data: {} ...\n", cli.value);
    println!("\n=== COLD QUERY ===");
    put_immutable(&dht, &value);

    println!("\n=== SUBSEQUENT QUERY ===");
    put_immutable(&dht, &value);
}

fn put_immutable(dht: &Dht, value: &Bytes) {
    let start = Instant::now();

    let info_hash = dht
        .put_immutable(value.to_owned())
        .expect("put immutable failed");

    println!(
        "Stored immutable data as {:?} in {:?} milliseconds",
        info_hash,
        start.elapsed().as_millis()
    );
}
