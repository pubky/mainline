#![doc = include_str!("../README.md")]
#![allow(unused)]

//! # Mainline
//! Rust implementation of read-only BitTorrent Mainline DHT client.

// Public modules
#[cfg(feature = "async")]
pub mod async_dht;
pub mod common;
pub mod dht;
pub mod error;
pub mod messages;
pub mod rpc;

pub use crate::common::*;
pub use crate::error::Error;
pub use crate::rpc::response::*;
pub use dht::{Dht, Testnet};

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
