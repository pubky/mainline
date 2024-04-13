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

    match Id::from_str(cli.infohash.as_str()) {
        Ok(infohash) => {
            let dht = Dht::default();

            let start = Instant::now();

            println!("\nAnnouncing peer on an infohash: {} ...\n", cli.infohash);

            let metadata = dht
                .announce_peer(infohash, Some(6991))
                .expect("announce_peer fialed");
            println!(
                "Announced peer in {:?} seconds",
                start.elapsed().as_secs_f32()
            );
            let stored_at = metadata.stored_at();
            println!("Stored at: {:?} nodes", stored_at.len());
            for node in stored_at {
                println!("   {:?}", node);
            }

            // You can now reannounce to the same closest nodes
            // skipping the the lookup step.
            //
            // This time we choose to not sepcify the port, effectively
            // making the port implicit to be detected by the storing node
            // from the source address of the announce_peer request
            //
            // Uncomment the following lines to try it out:

            // println!(
            //     "Announcing again to {:?} closest_nodes ...",
            //     metadata.closest_nodes().len()
            // );
            //
            // let again = Instant::now();
            // match dht.announce_peer_to(infohash, metadata.closest_nodes(), None) {
            //     Ok(metadata) => {
            //         println!(
            //             "Announced again to {:?} nodes in {:?} seconds",
            //             metadata.stored_at().len(),
            //             again.elapsed().as_secs()
            //         );
            //     }
            //     Err(err) => {
            //         println!("Error: {:?}", err);
            //     }
            // }
        }
        Err(err) => {
            println!("Error: {:?}", err)
        }
    };
}
