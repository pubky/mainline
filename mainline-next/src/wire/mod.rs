//! Serde representations of IPv4 KRPC messages.
//!
//! These types describe wire fields only. They do not apply routing, identity,
//! item-validation, or outgoing client policy, or bound bencode parsing.
//! Unknown fields are ignored; `y` and `q` select message and query kinds.

mod byte_array;
mod byte_string;
mod compact_address;
pub(crate) mod compact_nodes;
mod error_code;
mod id;
mod map;
mod message;
mod optional_bool;
pub(crate) mod query;
pub(crate) mod response;

pub(crate) use byte_array::ByteArray;
pub(crate) use byte_string::ByteString;
pub(crate) use compact_address::CompactAddress;
pub(crate) use compact_nodes::CompactNodes;
pub(crate) use error_code::ErrorCode;
pub(crate) use id::Id;
pub(crate) use map::Map;
// Used outside this module once the codec is added.
#[allow(unused_imports)]
pub(crate) use message::{WireKind, WireMessage};
pub(crate) use optional_bool::OptionalBool;
pub(crate) use query::WireQuery;
pub(crate) use response::ResponseArguments;

#[cfg(test)]
mod tests;
