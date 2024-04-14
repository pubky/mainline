#![doc = include_str!("../README.md")]

//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

// Public modules
pub mod common;
pub mod dht;
pub mod error;
pub mod messages;
pub mod rpc;

pub use crate::common::*;
pub use crate::error::Error;
pub use crate::rpc::Rpc;
pub use dht::{Dht, DhtSettings, Testnet};

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
