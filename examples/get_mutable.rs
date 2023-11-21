use mainline::{Error, Result};
use std::convert::TryInto;

use std::time::Instant;

use mainline::Dht;

use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Mutable data public key.
    public_key: String,
}

fn main() {
    let cli = Cli::parse();

    match from_hex(cli.public_key.clone()) {
        Ok(public_key) => {
            let dht = Dht::default();

            let start = Instant::now();
            let mut first = false;

            println!("\nLooking up mutable item: {} ...\n", cli.public_key);

            let mut count = 0;

            let mut response = &mut dht.get_mutable(public_key, None);

            for res in &mut response {
                if !first {
                    first = true;
                    println!(
                        "Got first result in {:?} seconds\n",
                        start.elapsed().as_secs_f32()
                    );

                    println!("Streaming mutable items:\n");
                }

                count += 1;

                match String::from_utf8(res.item.value.clone()) {
                    Ok(string) => {
                        println!(
                            "Got mutable item: {:?}, seq: {:?} | from: {:?}",
                            string, res.item.seq, res.from
                        );
                    }
                    Err(_) => {
                        println!(
                            "Got mutable item: {:?}, seq: {:?} | from: {:?}",
                            res.item.value, res.item.seq, res.from
                        );
                    }
                };
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

fn from_hex(s: String) -> Result<[u8; 32]> {
    if s.len() % 2 != 0 {
        return Err(Error::InvalidIdEncoding(
            "Number of Hex characters should be even".into(),
        ));
    }

    let mut bytes = Vec::with_capacity(s.len() / 2);

    for i in 0..s.len() / 2 {
        let byte_str = &s[i * 2..(i * 2) + 2];
        if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
            bytes.push(byte);
        } else {
            return Err(Error::Static("Invalid hex character")); // Invalid hex character
        }
    }

    let res: [u8; 32] = bytes.try_into().unwrap();

    Ok(res)
}
