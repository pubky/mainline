//! Complete-input decoding and bounded encoding of KRPC messages.

use serde::Deserialize;
use serde_bencode::value::Value;

use super::{bep44_value, bounds, CodecError};
use crate::wire::{WireKind, WireMessage, WireQuery};

/// Decode exactly one complete KRPC message within the datagram limit.
pub(crate) fn decode(bytes: &[u8]) -> Result<WireMessage, CodecError> {
    bounds::ensure_size(bytes.len())?;
    bounds::check_container_depth(bytes)?;

    let mut remaining = bytes;
    let message = {
        let mut deserializer = serde_bencode::Deserializer::new(&mut remaining);
        WireMessage::deserialize(&mut deserializer)?
    };

    ensure!(
        remaining.is_empty(),
        CodecError::TrailingData {
            remaining: remaining.len(),
        }
    );

    Ok(message)
}

/// Encode a KRPC message within the same size and depth limits as decoding.
pub(crate) fn encode(message: &WireMessage) -> Result<Vec<u8>, CodecError> {
    if let Some(value) = message_value(message) {
        bounds::check_value_nesting(value, 2)?; // Envelope and argument dictionaries.
        bep44_value::validate_outgoing_value_size(value)?;
    }
    let bytes = serde_bencode::to_bytes(message)?;
    bounds::ensure_size(bytes.len())?;
    Ok(bytes)
}

/// Return the BEP 44 `v` field when the message carries one.
fn message_value(message: &WireMessage) -> Option<&Value> {
    match &message.kind {
        WireKind::Query {
            query: WireQuery::Put { arguments },
            ..
        } => Some(&arguments.0.value),
        WireKind::Response { arguments, .. } => arguments.0.value.as_ref(),
        _ => None,
    }
}
