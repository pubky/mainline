use std::{str::FromStr, time::Instant};

use mainline::{Dht, Id};

use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Optional name to operate on
    infohash: String,
}

fn main() {
    let cli = Cli::parse();

    match Id::from_str(cli.infohash.as_str()) {
        Ok(infohash) => {
            let dht = Dht::new().unwrap();

            let start = Instant::now();

            println!("\nAnnouncing infohash: {} ...\n", cli.infohash);

            match dht.announce_peer(infohash, Some(6991)) {
                Ok(result) => {
                    println!(
                        "Announced peer in {:?} seconds",
                        start.elapsed().as_secs_f32()
                    );
                    let stored_at = result.stored_at();
                    println!("Stored at: {:?} nodes", stored_at.len());
                    for node in stored_at {
                        println!("   {:?}", node);
                    }
                }
                Err(err) => {
                    println!("Error: {:?}", err)
                }
            };
        }
        Err(err) => {
            println!("Error: {}", err)
        }
    };
}
