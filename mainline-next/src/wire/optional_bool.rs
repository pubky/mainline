use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An integer `0`/`1` flag. Use `#[serde(default)]` to treat absence as false.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OptionalBool(pub bool);

impl OptionalBool {
    pub(crate) fn is_false(&self) -> bool {
        !self.0
    }
}

impl Serialize for OptionalBool {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(i64::from(self.0))
    }
}

impl<'de> Deserialize<'de> for OptionalBool {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match i64::deserialize(deserializer)? {
            0 => Ok(Self(false)),
            1 => Ok(Self(true)),
            _ => Err(serde::de::Error::custom("expected integer 0 or 1")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_zero_and_one() {
        for (value, bytes) in [(false, b"i0e"), (true, b"i1e")] {
            let flag = OptionalBool(value);
            assert_eq!(serde_bencode::to_bytes(&flag).unwrap(), bytes);
            assert_eq!(
                serde_bencode::from_bytes::<OptionalBool>(bytes).unwrap(),
                flag
            );
        }
    }

    #[test]
    fn rejects_other_values() {
        for bytes in [b"i-1e".as_slice(), b"i2e", b"1:1", b"le", b"de"] {
            assert!(serde_bencode::from_bytes::<OptionalBool>(bytes).is_err());
        }
    }
}
