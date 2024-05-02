#![doc = include_str!("../README.md")]

// Public modules
mod common;
mod error;

#[cfg(feature = "async")]
pub mod async_dht;
pub mod dht;
pub mod rpc;
pub mod server;

pub use crate::common::{Id, MutableItem};
pub use bytes::Bytes;
pub use dht::{Dht, Testnet};

pub use ed25519_dalek::SigningKey;
pub use error::Error;

// Alias Result to be the crate Result.
pub type Result<T, E = error::Error> = core::result::Result<T, E>;
