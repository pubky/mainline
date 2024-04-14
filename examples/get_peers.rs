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
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();

    let info_hash = Id::from_str(cli.infohash.as_str()).expect("Expected info_hash");

    let dht = Dht::default();

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

    let receiver = dht.get_peers(*info_hash);

    while let Ok(peer) = receiver.recv() {
        if !first {
            first = true;
            println!(
                "Got first result in {:?} milliseconds\n",
                start.elapsed().as_millis()
            );

            println!("Streaming peers:\n");
        }

        count += 1;
        println!("Got peer {:?}", peer,);
    }

    println!(
        "\nQuery exhausted in {:?} milliseconds, got {:?} peers.",
        start.elapsed().as_millis(),
        count
    );
}
