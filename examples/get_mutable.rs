use ed25519_dalek::VerifyingKey;
use std::convert::TryFrom;
use tracing::Level;
use tracing_subscriber;

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
    tracing_subscriber::fmt()
        // Switch to DEBUG to see incoming values and the IP of the responding nodes
        .with_max_level(Level::INFO)
        .init();

    let cli = Cli::parse();

    let public_key = from_hex(cli.public_key.clone());
    let dht = Dht::client().unwrap();

    println!("Looking up mutable item: {} ...", cli.public_key);
    println!("\n=== COLD LOOKUP ===");
    lookup(&dht, public_key);

    println!("\n=== SUBSEQUENT LOOKUP ===");
    println!("Looking up mutable item: {} ...", cli.public_key);
    lookup(&dht, public_key);
}

fn lookup(dht: &Dht, public_key: VerifyingKey) {
    let start = Instant::now();
    let mut first = false;
    let mut count = 0;

    println!("Streaming mutable items:");
    for item in dht.get_mutable(public_key.as_bytes(), None, None).unwrap() {
        count += 1;

        if !first {
            first = true;
            println!(
                "\nGot first result in {:?} milliseconds:",
                start.elapsed().as_millis()
            );

            match String::from_utf8(item.value().to_vec()) {
                Ok(string) => {
                    println!("  mutable item: {:?}, seq: {:?}\n", string, item.seq());
                }
                Err(_) => {
                    println!(
                        "  mutable item: {:?}, seq: {:?}\n",
                        item.value(),
                        item.seq(),
                    );
                }
            };
        }
    }

    println!(
        "\nQuery exhausted in {:?} seconds, got {:?} values.",
        start.elapsed().as_secs_f32(),
        count
    );
}

fn from_hex(s: String) -> VerifyingKey {
    if s.len() % 2 != 0 {
        panic!("Number of Hex characters should be even");
    }

    let mut bytes = Vec::with_capacity(s.len() / 2);

    for i in 0..s.len() / 2 {
        let byte_str = &s[i * 2..(i * 2) + 2];
        let byte = u8::from_str_radix(byte_str, 16).expect("Invalid hex character");
        bytes.push(byte);
    }

    VerifyingKey::try_from(bytes.as_slice()).expect("Invalid mutable key")
}
