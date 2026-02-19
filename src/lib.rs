#![doc = include_str!("../README.md")]
//! ## Feature flags
#![doc = document_features::document_features!()]
//!

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

#[cfg(feature = "node")]
mod actor;
mod common;
#[cfg(feature = "node")]
mod core;
#[cfg(feature = "node")]
mod dht;

// Public modules
#[cfg(feature = "async")]
pub mod async_dht;

pub use common::{Id, MutableItem, Node, RoutingTable};

#[cfg(feature = "node")]
pub use actor::{
    messages::{MessageType, PutRequestSpecific, RequestSpecific},
    RequestFilter, ServerSettings, DEFAULT_REQUEST_TIMEOUT, MAX_INFO_HASHES, MAX_PEERS, MAX_VALUES,
};
#[cfg(feature = "node")]
pub use common::ClosestNodes;
#[cfg(feature = "node")]
pub use dht::{Dht, DhtBuilder, Testnet, TestnetBuilder};

pub use ed25519_dalek::SigningKey;

pub mod errors {
    //! Exported errors
    #[cfg(feature = "node")]
    pub use super::actor::{ConcurrencyError, PutError, PutQueryError};
    #[cfg(feature = "node")]
    pub use super::common::ErrorSpecific;
    #[cfg(feature = "node")]
    pub use super::dht::PutMutableError;

    pub use super::common::DecodeIdError;
    pub use super::common::MutableError;
}
