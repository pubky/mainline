//! Packet-size and nesting bounds for KRPC datagrams.

use serde_bencode::value::Value;

use super::CodecError;

/// Maximum accepted or emitted KRPC datagram size.
pub(super) const MAX_DATAGRAM_BYTES: usize = 2_048;
/// Maximum number of simultaneously open bencode lists and dictionaries.
pub(super) const MAX_NESTING_DEPTH: usize = 32;

pub(super) fn ensure_size(actual: usize) -> Result<(), CodecError> {
    ensure!(
        actual <= MAX_DATAGRAM_BYTES,
        CodecError::DatagramTooLarge {
            actual,
            maximum: MAX_DATAGRAM_BYTES,
        }
    );
    Ok(())
}

/// Reject item nesting before the bencode serializer recurses into it.
pub(super) fn check_value_nesting(value: &Value, parent_depth: usize) -> Result<(), CodecError> {
    if parent_depth >= MAX_NESTING_DEPTH && matches!(value, Value::List(_) | Value::Dict(_)) {
        return Err(CodecError::NestingTooDeep {
            maximum: MAX_NESTING_DEPTH,
        });
    }

    let depth = parent_depth + 1;
    match value {
        Value::List(values) => {
            for value in values {
                check_value_nesting(value, depth)?;
            }
        }
        Value::Dict(entries) => {
            for value in entries.values() {
                check_value_nesting(value, depth)?;
            }
        }
        Value::Bytes(_) | Value::Int(_) => {}
    }
    Ok(())
}

/// Check container depth without allocating or interpreting message semantics.
pub(super) fn check_container_depth(bytes: &[u8]) -> Result<(), CodecError> {
    let mut depth = 0;
    let mut position = 0;

    while position < bytes.len() {
        match bytes[position] {
            b'l' | b'd' => {
                depth += 1;
                ensure!(
                    depth <= MAX_NESTING_DEPTH,
                    CodecError::NestingTooDeep {
                        maximum: MAX_NESTING_DEPTH,
                    }
                );
                position += 1;
            }
            b'e' => {
                let Some(next_depth) = depth.checked_sub(1) else {
                    // Let the bencode backend report the malformed terminator.
                    return Ok(());
                };
                depth = next_depth;
                position += 1;
            }
            b'i' => {
                let Some(end) = bytes[position + 1..].iter().position(|&byte| byte == b'e') else {
                    // Let the bencode backend report the unterminated integer.
                    return Ok(());
                };
                position += end + 2;
            }
            b'0'..=b'9' => {
                let Some(end) = byte_string_end(bytes, position) else {
                    // Let the bencode backend report the malformed byte string.
                    return Ok(());
                };
                position = end;
            }
            _ => {
                // Let the bencode backend report the invalid token.
                return Ok(());
            }
        }
    }

    Ok(())
}

fn byte_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut length = 0_usize;
    let mut position = start;

    loop {
        match *bytes.get(position)? {
            digit @ b'0'..=b'9' => {
                length = length
                    .checked_mul(10)?
                    .checked_add(usize::from(digit - b'0'))?;
                position += 1;
            }
            b':' => {
                let payload_start = position + 1;
                let payload_end = payload_start.checked_add(length)?;
                return (payload_end <= bytes.len()).then_some(payload_end);
            }
            _ => return None,
        }
    }
}
