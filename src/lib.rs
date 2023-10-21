#![allow(unused)]
// Modules
mod error;
mod kademlia;

// Exports
pub use crate::error::Error;

// Alias Result to be the crate Result.
pub type Result<T, E = Error> = core::result::Result<T, E>;
