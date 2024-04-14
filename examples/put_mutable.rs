use std::{
    convert::TryFrom,
    time::{Instant, SystemTime},
};

use ed25519_dalek::SigningKey;
use mainline::{Dht, MutableItem};

use clap::Parser;

use tracing::Level;
use tracing_subscriber;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Mutable data public key.
    secret_key: String,
    /// Value to store on the DHT
    value: String,
}

fn main() {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cli = Cli::parse();

    let dht = Dht::default();

    let signer = from_hex(cli.secret_key);

    println!(
        "\nStoring mutable data: \"{}\" for public_key: {}  ...\n",
        cli.value,
        to_hex(signer.verifying_key().to_bytes().to_vec())
    );

    let seq = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time drift")
        .as_micros() as i64;

    let item = MutableItem::new(signer, cli.value.as_bytes().to_owned().into(), seq, None);

    println!("\n=== COLD QUERY ===");
    put(&dht, &item);

    println!("\n=== SUBSEQUENT QUERY ===");
    // You can now republish to the same closest nodes
    // skipping the the lookup step.
    put(&dht, &item);
}

fn put(dht: &Dht, item: &MutableItem) {
    let start = Instant::now();

    let metadata = dht.put_mutable(item.clone()).expect("put mutable failed");

    println!(
        "Stored mutable data as {:?} in {:?} milliseconds",
        metadata.target(),
        start.elapsed().as_millis()
    );

    let stored_at = metadata.stored_at();
    println!("Stored at: {:?} nodes", stored_at.len());
    for node in stored_at {
        println!("   {:?}", node);
    }
}

fn from_hex(s: String) -> SigningKey {
    if s.len() % 2 != 0 {
        panic!("Number of Hex characters should be even");
    }

    let mut bytes = Vec::with_capacity(s.len() / 2);

    for i in 0..s.len() / 2 {
        let byte_str = &s[i * 2..(i * 2) + 2];
        let byte = u8::from_str_radix(byte_str, 16).expect("Invalid hex character");
        bytes.push(byte);
    }

    SigningKey::try_from(bytes.as_slice()).expect("Invalid signing key")
}

fn to_hex(bytes: Vec<u8>) -> String {
    let hex_chars: String = bytes.iter().map(|byte| format!("{:02x}", byte)).collect();

    hex_chars
}
