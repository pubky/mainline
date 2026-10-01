//! Errors from bounded encoding and decoding.

use thiserror::Error;

use super::bep44_value::ItemValueError;

/// Failure to encode or decode a bounded KRPC datagram.
#[derive(Debug, Error)]
pub(crate) enum CodecError {
    #[error("datagram contains {actual} bytes, exceeding the {maximum}-byte limit")]
    DatagramTooLarge { actual: usize, maximum: usize },
    #[error("bencode nesting exceeds the depth limit of {maximum}")]
    NestingTooDeep { maximum: usize },
    #[error("datagram contains {remaining} trailing bytes")]
    TrailingData { remaining: usize },
    #[error(transparent)]
    Bep44Value(#[from] ItemValueError),
    #[error(transparent)]
    Bencode(#[from] serde_bencode::Error),
}
