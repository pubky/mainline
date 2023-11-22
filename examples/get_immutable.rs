use std::{str::FromStr, time::Instant};

use mainline::{Dht, Id};

use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Immutable data sha1 hash to lookup.
    target: String,
}

fn main() {
    let cli = Cli::parse();

    let target_parse_result: Result<Id, _> = Id::from_str(cli.target.as_str());

    match target_parse_result {
        Ok(infohash) => {
            let dht = Dht::default();

            let start = Instant::now();

            println!("\nLooking up immutable data: {} ...\n", cli.target);

            let mut response = &mut dht.get_immutable(infohash);

            if let Some(res) = response.next() {
                println!(
                    "Got result in {:?} seconds\n",
                    start.elapsed().as_secs_f32()
                );

                // No need to stream responses, just print the first result, since
                // all immutable data items are guaranteed to be the same.

                match String::from_utf8(res.value.to_vec()) {
                    Ok(string) => {
                        println!("Got immutable data: {:?} | from: {:?}", string, res.from);
                    }
                    Err(_) => {
                        println!("Got immutable data: {:?} | from: {:?}", res.value, res.from);
                    }
                };
            }

            println!(
                "\nVisited {:?} nodes, found {:?} closest nodes",
                response.visited,
                &response.closest_nodes.len()
            );

            println!(
                "\nQuery exhausted in {:?} seconds",
                start.elapsed().as_secs_f32(),
            );
        }
        Err(err) => {
            println!("Error: {}", err)
        }
    };
}
