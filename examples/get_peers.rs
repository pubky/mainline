use std::{str::FromStr, time::Instant};

use mainline::{Dht, Id};

use clap::Parser;

use tracing::Level;
use tracing_subscriber;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// info_hash to lookup peers for
    infohash: String,
}

fn main() {
    tracing_subscriber::fmt()
        // Switch to DEBUG to see incoming values and the IP of the responding nodes
        .with_max_level(Level::INFO)
        .init();

    let cli = Cli::parse();

    let info_hash = Id::from_str(cli.infohash.as_str()).expect("Expected info_hash");

    let dht = Dht::client().unwrap();

    println!("Looking up peers for info_hash: {} ...", info_hash);
    println!("\n=== COLD QUERY ===");
    get_peers(&dht, &info_hash);

    println!("\n=== SUBSEQUENT QUERY ===");
    println!("Looking up peers for info_hash: {} ...", info_hash);
    get_peers(&dht, &info_hash);
}

fn get_peers(dht: &Dht, info_hash: &Id) {
    let start = Instant::now();
    let mut first = false;

    let mut count = 0;

    for peer in dht.get_peers(*info_hash) {
        if !first {
            first = true;
            println!(
                "Got first result in {:?} milliseconds:",
                start.elapsed().as_millis()
            );

            println!("peer {:?}", peer,);
        }

        count += 1;
    }

    println!(
        "\nQuery exhausted in {:?} milliseconds, got {:?} peers.",
        start.elapsed().as_millis(),
        count
    );
}
