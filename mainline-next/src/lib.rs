//! Development crate for the Mainline DHT refactor.
//!
//! This unpublished crate is developed alongside `mainline`. It currently
//! provides internal KRPC wire types, with no runtime or public
//! API. See the repository's `mainline-next/docs` directory for the design.

// The codec and reactor will consume these wire types in subsequent steps.
#[allow(dead_code)]
mod wire;
