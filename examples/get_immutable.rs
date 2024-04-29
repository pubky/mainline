use std::{str::FromStr, time::Instant};

use mainline::{Dht, Id};

use clap::Parser;

use tracing::Level;
use tracing_subscriber;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Immutable data sha1 hash to lookup.
    target: String,
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();

    let info_hash = Id::from_str(cli.target.as_str()).expect("Invalid info_hash");

    let dht = Dht::client().unwrap();

    println!("\nLooking up immutable data: {} ...\n", cli.target);

    println!("\n=== COLD QUERY ===");
    get_immutable(&dht, info_hash);

    println!("\n=== SUBSEQUENT QUERY ===");
    get_immutable(&dht, info_hash);
}

fn get_immutable(dht: &Dht, info_hash: Id) {
    let start = Instant::now();

    // No need to stream responses, just print the first result, since
    // all immutable data items are guaranteed to be the same.
    let value = dht
        .get_immutable(info_hash)
        .expect("Failed to find the immutable value for the provided info_hash");

    let string = String::from_utf8(value.to_vec())
        .expect("expected immutable data to be valid utf-8 for this demo");

    println!(
        "Got result in {:?} milliseconds\n",
        start.elapsed().as_millis()
    );

    println!("Got immutable data: {:?}", string);

    println!(
        "\nQuery exhausted in {:?} milliseconds",
        start.elapsed().as_millis(),
    );
}
