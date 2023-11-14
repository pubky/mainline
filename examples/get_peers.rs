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
            let mut first = false;

            println!("\nLooking up infohash: {} ...\n", cli.infohash);

            let mut count = 0;

            let response = &mut dht.get_peers(infohash);

            for value in response {
                if !first {
                    first = true;
                    println!("Got first result in {:?}\n", start.elapsed().as_secs_f32());

                    println!("Streaming peers:\n");
                }

                count += 1;
                println!("Got peer {:?} | from: {:?}", value.peer, value.from);
            }

            println!(
                "\nQuery exhausted in {:?} seconds, got {:?} peers.",
                start.elapsed().as_secs_f32(),
                count
            );
        }
        Err(err) => {
            println!("Error: {}", err)
        }
    };
}
