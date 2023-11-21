#![doc = include_str!("../README.md")]
#![allow(unused)]

//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

// Public modules
pub mod async_dht;
/// Miscellaneous common structs used throughout the library.
pub mod common;
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
pub use dht::{Dht, Testnet};

// Re-exports
pub use crate::error::Error;
pub use ed25519_dalek;

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
