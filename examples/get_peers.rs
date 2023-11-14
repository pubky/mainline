use std::{convert::TryInto, time::Instant};

use mainline::{Dht, Id};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Optional name to operate on
    infohash: String,
}

#[derive(Subcommand)]
enum Commands {
    /// does testing things
    Test {
        /// lists test values
        #[arg(short, long)]
        list: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    let infohash_parse_result: Result<Id, _> = cli.infohash.as_str().try_into();

    match infohash_parse_result {
        Ok(infohash) => {
            dbg!(infohash);

            let dht = Dht::new().unwrap();

            let start = Instant::now();
            let mut first = false;

            println!("\nLooking up infohash: {}...\n", cli.infohash);

            for peer_response in dht.get_peers(infohash) {
                if !first {
                    first = true;
                    println!("Got first result in {:?}\n", start.elapsed().as_secs_f32());

                    println!("Streaming peers:\n");
                }

                println!(
                    "Got peer {:?} | from node: {:?}",
                    peer_response.peer, peer_response.from
                );
            }

            println!("\nQuery exhausted in {:?}", start.elapsed().as_secs_f32());
        }
        Err(err) => {
            println!("Error: {}", err)
        }
    };
}