#![doc = include_str!("../README.md")]
//! ## Feature flags
#![doc = document_features::document_features!()]
//!

mod common;
#[cfg(feature = "node")]
mod dht;

// Public modules
#[cfg(feature = "async")]
pub mod async_dht;
pub mod rpc;
pub mod server;

pub use crate::common::{Id, MutableItem, Node, RoutingTable};

#[cfg(feature = "node")]
pub use dht::{Config, Dht, Testnet};

pub use ed25519_dalek::SigningKey;

pub mod errors {
    //! Exported errors
    #[cfg(feature = "node")]
    pub use super::dht::{AnnouncePeerError, DhtWasShutdown, PutImmutableError, PutMutableError};

    pub use super::rpc::{ConcurrencyError, PutError, PutQueryError};
}
