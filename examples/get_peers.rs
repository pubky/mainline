use std::{str::FromStr, time::Instant};

use mainline::{Dht, Id};

use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// info_hash to lookup peers for
    infohash: String,
}

fn main() {
    let cli = Cli::parse();

    let infohash_parse_result: Result<Id, _> = Id::from_str(cli.infohash.as_str());

    match infohash_parse_result {
        Ok(infohash) => {
            let dht = Dht::default();

            let start = Instant::now();
            let mut first = false;

            println!("\nLooking up infohash: {} ...\n", cli.infohash);

            let mut count = 0;

            let mut response = &mut dht.get_peers(infohash);

            for item in &mut response {
                if !first {
                    first = true;
                    println!(
                        "Got first result in {:?} seconds\n",
                        start.elapsed().as_secs_f32()
                    );

                    println!("Streaming peers:\n");
                }

                count += 1;
                println!("Got peer {:?} | from: {:?}", item.peer, item.from);
            }

            println!(
                "Visited {:?} nodes, found {:?} closest nodes",
                response.visited,
                &response.closest_nodes.len()
            );

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
