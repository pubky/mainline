use std::net::{Ipv4Addr, SocketAddrV4};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ByteArray;

/// IPv4 address and port encoded as six compact bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompactAddress(pub SocketAddrV4);

impl CompactAddress {
    pub(crate) fn to_bytes(self) -> [u8; 6] {
        let mut bytes = [0; 6];
        bytes[..4].copy_from_slice(&self.0.ip().octets());
        bytes[4..].copy_from_slice(&self.0.port().to_be_bytes());
        bytes
    }

    pub(crate) fn from_bytes([a, b, c, d, high, low]: [u8; 6]) -> Self {
        Self(SocketAddrV4::new(
            Ipv4Addr::new(a, b, c, d),
            u16::from_be_bytes([high, low]),
        ))
    }
}

impl Serialize for CompactAddress {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.to_bytes())
    }
}

impl<'de> Deserialize<'de> for CompactAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        ByteArray::<6>::deserialize(deserializer).map(|bytes| Self::from_bytes(bytes.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_address_uses_network_byte_order() {
        let address = CompactAddress(SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 6881));
        let bytes = b"6:\x01\x02\x03\x04\x1a\xe1";
        assert_eq!(serde_bencode::to_bytes(&address).unwrap(), bytes);
        assert_eq!(
            serde_bencode::from_bytes::<CompactAddress>(bytes).unwrap(),
            address
        );
    }

    #[test]
    fn compact_address_requires_six_byte_string() {
        for bytes in [
            b"0:".as_slice(),
            b"5:abcde",
            b"7:abcdefg",
            b"li1ei2ei3ei4ei5ei6ee",
        ] {
            assert!(serde_bencode::from_bytes::<CompactAddress>(bytes).is_err());
        }
    }
}
