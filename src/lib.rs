#![doc = include_str!("../README.md")]
//! ## Feature flags
#![doc = document_features::document_features!()]
//!

#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

mod common;
#[cfg(feature = "node")]
mod dht;

// Public modules
#[cfg(feature = "async")]
pub mod async_dht;
pub(crate) mod rpc;
pub mod server;

pub use rpc::DEFAULT_REQUEST_TIMEOUT;

pub use crate::common::{Id, MutableItem, Node, PutRequestSpecific, RoutingTable};

#[cfg(feature = "node")]
pub use dht::{Dht, DhtBuilder, Testnet};

pub use ed25519_dalek::SigningKey;

pub mod errors {
    //! Exported errors
    #[cfg(feature = "node")]
    pub use super::common::ErrorSpecific;
    #[cfg(feature = "node")]
    pub use super::dht::{AnnouncePeerError, DhtWasShutdown, PutImmutableError, PutMutableError};
    #[cfg(feature = "node")]
    pub use super::rpc::{ConcurrencyError, PutError, PutQueryError};

    pub use super::common::DecodeIdError;
    pub use super::common::MutableError;
}
