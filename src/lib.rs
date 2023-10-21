#![allow(unused)]
//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

/// Miscellaneous common structs used throughout the library.
pub mod common;
/// Errors
pub mod error;
pub mod messages;
/// Kademlia routing table to keep track of local view of the Mainline network.
pub mod routing_table;
pub mod rpc;

// Re-exports
pub use crate::error::Error;

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
