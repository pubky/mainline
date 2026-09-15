use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An integer flag where zero is false and any nonzero value is true.
///
/// Use `#[serde(default)]` to treat absence as false. Serialization uses the
/// canonical values `0` and `1`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WireBool(pub bool);

impl WireBool {
    pub(crate) fn is_false(&self) -> bool {
        !self.0
    }
}

impl Serialize for WireBool {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(i64::from(self.0))
    }
}

impl<'de> Deserialize<'de> for WireBool {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self(i64::deserialize(deserializer)? != 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_canonical_zero_and_one() {
        for (value, bytes) in [(false, b"i0e"), (true, b"i1e")] {
            let flag = WireBool(value);
            assert_eq!(serde_bencode::to_bytes(&flag).unwrap(), bytes);
            assert_eq!(serde_bencode::from_bytes::<WireBool>(bytes).unwrap(), flag);
        }
    }

    #[test]
    fn decodes_any_nonzero_integer_as_true() {
        for bytes in [b"i-1e".as_slice(), b"i1e", b"i2e"] {
            assert_eq!(
                serde_bencode::from_bytes::<WireBool>(bytes).unwrap(),
                WireBool(true)
            );
        }
    }

    #[test]
    fn rejects_non_integer_values() {
        for bytes in [b"1:1".as_slice(), b"le", b"de"] {
            assert!(serde_bencode::from_bytes::<WireBool>(bytes).is_err());
        }
    }
}
