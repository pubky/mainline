#![allow(unused)]
//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

/// Miscellaneous common structs used throughout the library.
mod common;
pub mod dht;
/// Errors
mod error;
mod messages;
/// Kademlia routing table to keep track of local view of the Mainline network.
mod routing_table;
mod rpc;

// Re-exports
pub use crate::error::Error;

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
