#![doc = include_str!("../README.md")]
//! ## Feature flags
#![doc = document_features::document_features!()]
//!

// Public modules
mod common;
mod dht;

// Public modules
#[cfg(feature = "async")]
pub mod async_dht;
pub mod rpc;
pub mod server;

pub use crate::common::{Id, MutableItem, Node};
pub use dht::{Dht, Settings, Testnet};
pub use rpc::ClosestNodes;

pub use bytes::Bytes;
pub use ed25519_dalek::SigningKey;

pub mod errors {
    //! Exported errors
    pub use super::rpc::PutError;
}
