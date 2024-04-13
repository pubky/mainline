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
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();

    let public_key = from_hex(cli.public_key.clone());
    let dht = Dht::default();

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

    let receiver = dht.get_mutable(public_key.as_bytes(), None);

    println!("Streaming mutable items:");
    while let Ok(item) = receiver.recv() {
        if !first {
            first = true;
            println!(
                "\nGot first result in {:?} milliseconds\n",
                start.elapsed().as_millis()
            );
        }

        count += 1;

        {
            match String::from_utf8(item.value().to_vec()) {
                Ok(string) => {
                    println!("Got mutable item: {:?}, seq: {:?}", string, item.seq());
                }
                Err(_) => {
                    println!(
                        "Got mutable item: {:?}, seq: {:?}",
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
