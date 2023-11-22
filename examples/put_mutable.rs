use std::{
    convert::TryFrom,
    time::{Instant, SystemTime},
};

use ed25519_dalek::SigningKey;
use mainline::{common::MutableItem, Dht};

use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Mutable data public key.
    secret_key: String,
    /// Value to store on the DHT
    value: String,
}

fn main() {
    let cli = Cli::parse();

    let dht = Dht::default();

    let start = Instant::now();

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

    let metadata = dht.put_mutable(item).expect("put mutable failed");

    println!(
        "Stored immutable data as {:?} in {:?} seconds",
        metadata.target(),
        start.elapsed().as_secs_f32()
    );

    let stored_at = metadata.stored_at();
    println!("Stored at: {:?} nodes", stored_at.len());
    for node in stored_at {
        println!("   {:?}", node);
    }

    // You can now republish to the same closest nodes
    // skipping the the lookup step.
    //
    // This time we choose to not sepcify the port, effectively
    // making the port implicit to be detected by the storing node
    // from the source address of the put_immutable request
    //
    // Uncomment the following lines to try it out:

    // println!(
    //     "Publishing immutable data again to {:?} closest_nodes ...",
    //     metadata.closest_nodes().len()
    // );
    //
    // let again = Instant::now();
    // match dht.put_immutable_to(target, value, metadata.closest_nodes()) {
    //     Ok(metadata) => {
    //         println!(
    //             "Published again to {:?} nodes in {:?} seconds",
    //             metadata.stored_at().len(),
    //             again.elapsed().as_secs()
    //         );
    //     }
    //     Err(err) => {
    //         println!("Error: {:?}", err);
    //     }
    // }
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
