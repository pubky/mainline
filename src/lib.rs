#![doc = include_str!("../README.md")]

//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

// Public modules
#[cfg(feature = "async")]
mod async_dht;
mod common;
mod dht;
mod error;
mod messages;
mod rpc;

pub use crate::common::*;
pub use crate::error::Error;
pub use dht::{Dht, DhtSettings, Testnet};
pub use rpc::Rpc;

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
