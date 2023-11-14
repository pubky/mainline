#![allow(unused)]
//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

// Public modules
/// Miscellaneous common structs used throughout the library.
mod common;
pub mod dht;

/// Errors
mod error;
mod messages;
mod peers;
mod query;
mod routing_table;
mod rpc;
mod socket;
mod tokens;

pub use crate::common::Id;
pub use dht::Dht;

// Re-exports
pub use crate::error::Error;

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
