//! Development crate for the Mainline DHT refactor.
//!
//! This unpublished crate is developed alongside `mainline`.

/// Return a typed error when a validation condition is false.
macro_rules! ensure {
    ($condition:expr, $error:expr $(,)?) => {
        if !$condition {
            return Err($error);
        }
    };
}

#[allow(dead_code)]
mod codec;
mod distance;
#[allow(dead_code)]
mod reactor;
pub use reactor::{ReactorHandle, ShutdownError};
#[allow(dead_code)]
mod bep42;

#[allow(dead_code)]
mod wire;
