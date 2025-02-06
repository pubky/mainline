use std::{convert::TryFrom, time::Instant};

use mainline::{Dht, MutableItem, SigningKey};

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

    let dht = Dht::client().unwrap();

    let signer = from_hex(cli.secret_key);

    println!(
        "\nStoring mutable data: \"{}\" for public_key: {}  ...",
        cli.value,
        to_hex(signer.verifying_key().as_bytes())
    );

    println!("\n=== COLD QUERY ===");
    put(&dht, &signer, cli.value.as_bytes(), None);

    println!("\n=== SUBSEQUENT QUERY ===");
    // You can now republish to the same closest nodes
    // skipping the the lookup step.
    put(&dht, &signer, cli.value.as_bytes(), None);
}

fn put(dht: &Dht, signer: &SigningKey, value: &[u8], salt: Option<&[u8]>) {
    let start = Instant::now();

    let (item, cas) = if let Some(most_recent) =
        dht.get_mutable_most_recent(signer.verifying_key().as_bytes(), salt)
    {
        // 1. Optionally Create a new value to take the most recent's value in consideration.
        let mut new_value = most_recent.value().to_vec();

        println!(
            "Found older value {:?}, appending new value to the old...",
            new_value
        );

        new_value.extend_from_slice(value);

        // 2. Increment the sequence number to be higher than the most recent's.
        let most_recent_seq = most_recent.seq();
        let new_seq = most_recent_seq + 1;

        println!("Found older seq {most_recent_seq} incremnting sequence to {new_seq}...",);

        (
            MutableItem::new(signer.clone(), &new_value, new_seq, salt),
            // 3. Use the most recent [MutableItem::seq] as a `CAS`.
            Some(most_recent_seq),
        )
    } else {
        (MutableItem::new(signer.clone(), value, 1, salt), None)
    };

    dht.put_mutable(item, cas).unwrap();

    println!(
        "Stored mutable data as {:?} in {:?} milliseconds",
        to_hex(signer.verifying_key().as_bytes()),
        start.elapsed().as_millis()
    );
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

fn to_hex(bytes: &[u8]) -> String {
    let hex_chars: String = bytes.iter().map(|byte| format!("{:02x}", byte)).collect();

    hex_chars
}
