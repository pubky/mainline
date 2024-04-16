use std::{str::FromStr, time::Instant};

use mainline::{Dht, Id};

use clap::Parser;

use tracing::Level;
use tracing_subscriber;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// info_hash to annouce a peer on
    infohash: String,
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();

    let info_hash = Id::from_str(cli.infohash.as_str()).expect("invalid infohash");

    let dht = Dht::client().unwrap();

    println!("\nAnnouncing peer on an infohash: {} ...\n", cli.infohash);

    println!("\n=== COLD QUERY ===");
    announce(&dht, info_hash);

    println!("\n=== SUBSEQUENT QUERY ===");
    announce(&dht, info_hash);
}

fn announce(dht: &Dht, info_hash: Id) {
    let start = Instant::now();

    dht.announce_peer(info_hash, Some(6991))
        .expect("announce_peer failed");

    println!(
        "Announced peer in {:?} seconds",
        start.elapsed().as_secs_f32()
    );
}
