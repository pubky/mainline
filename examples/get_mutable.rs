use ed25519_dalek::VerifyingKey;
use std::convert::TryFrom;
use tracing::Level;
use tracing_subscriber;

use std::time::Instant;

use mainline::{Dht, MutableItem};

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

    let public_key = from_hex(cli.public_key.clone()).to_bytes();
    let dht = Dht::client().unwrap();

    println!("Looking up mutable item: {} ...", cli.public_key);

    println!("\n=== COLD LOOKUP ===");
    get_first(&dht, &public_key);

    println!("\n=== SUBSEQUENT LOOKUP ===");
    get_first(&dht, &public_key);

    println!("\n=== GET MOST RECENT ===");
    let start = Instant::now();

    println!("\nLooking up the most recent value..");
    let item = dht.get_mutable_most_recent(&public_key, None).unwrap();

    if let Some(item) = item {
        println!("Found the most recent value:");
        print_value(&item);
    } else {
        println!("Not found");
    }

    println!(
        "\nQuery exhausted in {:?} seconds.",
        start.elapsed().as_secs_f32(),
    );
}

fn get_first(dht: &Dht, public_key: &[u8; 32]) {
    let start = Instant::now();
    if let Some(item) = dht.get_mutable(public_key, None, None).unwrap().next() {
        println!(
            "\nGot first result in {:?} milliseconds:",
            start.elapsed().as_millis()
        );
        print_value(&item);
    } else {
        println!("Not Found")
    }
}

fn print_value(item: &MutableItem) {
    match String::from_utf8(item.value().to_vec()) {
        Ok(string) => {
            println!("  mutable item: {:?}, seq: {:?}", string, item.seq());
        }
        Err(_) => {
            println!("  mutable item: {:?}, seq: {:?}", item.value(), item.seq(),);
        }
    };
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
