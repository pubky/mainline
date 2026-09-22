//! Development crate for the Mainline DHT refactor.
//!
//! This unpublished crate is developed alongside `mainline`.

mod distance;
#[allow(dead_code)]
mod reactor;
pub use reactor::{ReactorHandle, ShutdownError};
#[allow(dead_code)]
mod bep42;

// The codec and reactor will consume these wire types in subsequent steps.
#[allow(dead_code)]
mod wire;
