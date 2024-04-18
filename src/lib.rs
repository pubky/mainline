#![doc = include_str!("../README.md")]

//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

// Public modules
mod common;
mod error;

#[cfg(feature = "async")]
pub mod async_dht;
pub mod dht;
pub mod rpc;
pub mod server;

pub use crate::common::{messages, Id, MutableItem, RoutingTable};
pub use dht::Dht;
pub use error::Error;

// Alias Result to be the crate Result.
pub type Result<T, E = error::Error> = core::result::Result<T, E>;
