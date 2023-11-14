use std::{convert::TryInto, time::Instant};

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

    let infohash_parse_result: Result<Id, _> = cli.infohash.as_str().try_into();

    match infohash_parse_result {
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
                    println!("Announced to: {:?} nodes", result.success.len());
                    println!(
                        "Stored at: {:?}",
                        result
                            .closest_nodes
                            .iter()
                            .filter(|node| result.success.contains(&node.id))
                            .collect::<Vec<_>>()
                    );
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
