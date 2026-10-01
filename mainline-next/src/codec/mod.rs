//! Bounded encoding and complete-input decoding for KRPC datagrams.

mod bep44_value;
mod bounds;
mod datagram;
mod error;
#[cfg(test)]
mod tests;

// The reactor will consume these entry points in a later step.
#[allow(unused_imports)]
pub(crate) use datagram::{decode, encode};
pub(crate) use error::CodecError;
