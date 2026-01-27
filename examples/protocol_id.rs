//! Example demonstrating network isolation using protocol IDs.
//!
//! This example shows how to create isolated DHT networks that don't interfere
//! with each other using the protocol ID.
//!
//! Run with:
//! ```bash
//! cargo run --example protocol_id
//! ```

use mainline::{Dht, DEFAULT_STAGING_PROTOCOL_ID};

fn main() -> Result<(), std::io::Error> {
    println!("Protocol ID Example\n");

    // Example 1: Default behavior - participates in main BitTorrent DHT
    println!("1. Creating DHT node without protocol ID (default behavior):");
    println!("   This node will communicate with the main BitTorrent DHT network");
    let _default_dht = Dht::client()?;
    println!("   ✓ Created\n");

    // Example 2: Using a custom protocol ID for an isolated network
    println!("2. Creating DHT node with custom protocol ID:");
    println!("   Protocol ID: /myapp/mainline/1.0.0");
    println!("   This node will only communicate with other nodes using the same protocol ID");
    let _custom_dht = Dht::builder()
        .protocol_id("/myapp/mainline/1.0.0")
        .build()?;
    println!("   ✓ Created\n");

    // Example 3: Using the default staging protocol ID constant
    println!("4. Creating DHT node using DEFAULT_STAGING_PROTOCOL_ID:");
    println!("   Protocol ID: {}", DEFAULT_STAGING_PROTOCOL_ID);
    println!("   Useful for creating isolated test networks");
    let _staging_dht = Dht::builder()
        .protocol_id(DEFAULT_STAGING_PROTOCOL_ID)
        .build()?;
    println!("   ✓ Created\n");

    println!("Network Isolation Rules:");
    println!("• Nodes with the same protocol ID can communicate");
    println!("• Nodes with different protocol IDs ignore each other's messages");
    println!("• Nodes without a protocol ID (None) accept all messages (backward compatible)");
    println!("• Nodes with a protocol ID ONLY accept messages with matching protocol ID");

    Ok(())
}
