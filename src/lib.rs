#![doc = include_str!("../README.md")]

//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

// Public modules
#[cfg(feature = "async")]
mod async_dht;
mod common;
mod dht;
mod error;
mod rpc;

#[cfg(feature = "async")]
pub use crate::async_dht::AsyncDht;
pub use crate::common::{messages, Id, MutableItem};
pub use crate::error::Error;
pub use dht::{Dht, DhtSettings, Testnet};
pub use rpc::Rpc;

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
