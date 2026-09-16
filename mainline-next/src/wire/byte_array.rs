use std::fmt;

use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

/// A fixed-size binary string, without cryptographic validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ByteArray<const N: usize>(pub [u8; N]);

impl<const N: usize> Serialize for ByteArray<N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de, const N: usize> Deserialize<'de> for ByteArray<N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor<const N: usize>;

        impl<const N: usize> Visitor<'_> for BytesVisitor<N> {
            type Value = ByteArray<N>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "a {N}-byte string")
            }

            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                bytes
                    .try_into()
                    .map(ByteArray)
                    .map_err(|_error| E::invalid_length(bytes.len(), &self))
            }
        }

        deserializer.deserialize_bytes(BytesVisitor::<N>)
    }
}

#[cfg(test)]
mod tests {
    use serde_bencode::value::Value;

    use super::*;

    #[test]
    fn encodes_bytes_and_requires_exact_length() {
        fn check<const N: usize>() {
            let bytes = ByteArray([255; N]);
            let encoded = serde_bencode::to_bytes(&bytes).unwrap();
            let prefix = format!("{N}:");
            assert_eq!(&encoded[..prefix.len()], prefix.as_bytes());
            assert_eq!(&encoded[prefix.len()..], &bytes.0);
            assert_eq!(
                serde_bencode::from_bytes::<ByteArray<N>>(&encoded).unwrap(),
                bytes
            );
            for value in [
                Value::Bytes(vec![]),
                Value::Bytes(vec![0; N - 1]),
                Value::Bytes(vec![0; N + 1]),
                Value::List(vec![Value::Int(0); N]),
            ] {
                assert!(serde_bencode::from_bytes::<ByteArray<N>>(
                    &serde_bencode::to_bytes(&value).unwrap()
                )
                .is_err());
            }
        }

        check::<20>();
        check::<32>();
        check::<64>();
    }
}
