#![doc = include_str!("../README.md")]
//! ## Feature flags
#![doc = document_features::document_features!()]
//!

// Public modules
mod common;

#[cfg(feature = "async")]
pub mod async_dht;
mod dht;
pub mod rpc;
pub mod server;

pub use crate::common::{Id, MutableItem};
pub use bytes::Bytes;
pub use dht::{Dht, Testnet};

pub use ed25519_dalek::SigningKey;

mod errors {
    pub use super::rpc::PutError;
}
