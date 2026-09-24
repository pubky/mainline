//! BEP 44 size validation for values carried in KRPC messages.

use serde_bencode::value::Value;
use thiserror::Error;

/// Maximum size of the complete bencoded `v` value, including type markers.
pub(super) const MAX_BEP44_VALUE_BYTES: usize = 1_000;

/// Validate the encoded size of any BEP 44 value about to be sent.
pub(super) fn validate_outgoing_value_size(value: &Value) -> Result<(), ItemValueError> {
    // The wire codec checks nesting before calling this function.
    let encoded = serde_bencode::to_bytes(value)?;
    validate_encoded_value_size(&encoded)
}

/// Validate the size of a complete bencoded `v` value.
///
/// Outgoing values can use their serialized bytes. Future incoming item
/// validation must pass the original `v` bytes, not a re-encoding of a decoded
/// `Value`: re-encoding can remove duplicate keys and normalize bytes.
pub(super) fn validate_encoded_value_size(encoded_value: &[u8]) -> Result<(), ItemValueError> {
    ensure!(
        encoded_value.len() <= MAX_BEP44_VALUE_BYTES,
        ItemValueError::TooLarge {
            actual: encoded_value.len(),
            maximum: MAX_BEP44_VALUE_BYTES,
        }
    );
    Ok(())
}

#[derive(Debug, Error)]
pub(crate) enum ItemValueError {
    #[error("BEP 44 value has {actual} encoded bytes, exceeding the {maximum}-byte limit")]
    TooLarge { actual: usize, maximum: usize },
    #[error(transparent)]
    Bencode(#[from] serde_bencode::Error),
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_bencode::value::Value;

    use super::*;

    #[test]
    fn checks_the_complete_bencoding_of_each_value_type() {
        for value in [
            Value::Bytes(vec![0; 996]),
            Value::List(vec![Value::Bytes(vec![0; 994])]),
            Value::Dict(HashMap::from([(b"k".to_vec(), Value::Bytes(vec![0; 991]))])),
            Value::Int(i64::MAX),
        ] {
            let encoded = serde_bencode::to_bytes(&value).unwrap();
            assert!(encoded.len() <= MAX_BEP44_VALUE_BYTES);
            validate_encoded_value_size(&encoded).unwrap();
        }
    }

    #[test]
    fn rejects_values_above_the_limit() {
        for value in [
            Value::Bytes(vec![0; 997]),
            Value::List(vec![Value::Bytes(vec![0; 995])]),
            Value::Dict(HashMap::from([(b"k".to_vec(), Value::Bytes(vec![0; 992]))])),
        ] {
            let encoded = serde_bencode::to_bytes(&value).unwrap();
            assert_eq!(encoded.len(), MAX_BEP44_VALUE_BYTES + 1);
            assert!(matches!(
                validate_encoded_value_size(&encoded),
                Err(ItemValueError::TooLarge {
                    actual,
                    maximum: MAX_BEP44_VALUE_BYTES,
                }) if actual == encoded.len()
            ));
        }
    }

    #[test]
    fn received_size_uses_original_bytes_not_a_normalized_value() {
        let mut encoded = vec![b'd'];
        for _ in 0..167 {
            encoded.extend_from_slice(b"1:ai1e");
        }
        encoded.push(b'e');

        let normalized: Value = serde_bencode::from_bytes(&encoded).unwrap();
        assert!(serde_bencode::to_bytes(&normalized).unwrap().len() < MAX_BEP44_VALUE_BYTES);
        assert!(matches!(
            validate_encoded_value_size(&encoded),
            Err(ItemValueError::TooLarge { actual, .. }) if actual == encoded.len()
        ));
    }
}
