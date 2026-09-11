use std::fmt;

use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

/// Accept binary strings only; byte-vector visitors may also accept integer lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ByteString(pub Vec<u8>);

impl Serialize for ByteString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for ByteString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;

        impl Visitor<'_> for BytesVisitor {
            type Value = ByteString;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a byte string")
            }

            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                Ok(ByteString(bytes.to_vec()))
            }

            fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Self::Value, E> {
                Ok(ByteString(bytes))
            }
        }

        deserializer.deserialize_byte_buf(BytesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialization_reuses_owned_buffer() {
        struct OwnedBytes(Vec<u8>);

        impl<'de> Deserializer<'de> for OwnedBytes {
            type Error = serde::de::value::Error;

            fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
                visitor.visit_byte_buf(self.0)
            }

            serde::forward_to_deserialize_any! {
                bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes
                byte_buf option unit unit_struct newtype_struct seq tuple
                tuple_struct map struct enum identifier ignored_any
            }
        }

        let bytes = vec![0, 255, 128];
        let pointer = bytes.as_ptr();
        let decoded = ByteString::deserialize(OwnedBytes(bytes)).unwrap();
        assert_eq!(decoded.0, [0, 255, 128]);
        assert_eq!(decoded.0.as_ptr(), pointer);
    }

    #[test]
    fn byte_string_rejects_integer_lists() {
        assert!(serde_bencode::from_bytes::<ByteString>(b"li1ei2ee").is_err());
    }
}
