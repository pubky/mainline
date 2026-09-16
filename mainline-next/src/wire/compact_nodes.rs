use std::{fmt, net::SocketAddrV4};

use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};

use super::{ByteArray, CompactAddress, Id};

const CONTACT_BYTES: usize = 26;

/// A node's advertised ID and address, without routing state or verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NodeContact {
    pub id: Id,
    pub address: SocketAddrV4,
}

/// Node contacts concatenated into one binary string, 26 bytes per contact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompactNodes(pub Vec<NodeContact>);

impl Serialize for CompactNodes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut bytes = Vec::with_capacity(self.0.len() * CONTACT_BYTES);
        for contact in &self.0 {
            bytes.extend_from_slice(&contact.id.0);
            bytes.extend_from_slice(&CompactAddress(contact.address).to_bytes());
        }
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for CompactNodes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct NodesVisitor;

        impl Visitor<'_> for NodesVisitor {
            type Value = CompactNodes;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a byte string of 26-byte IPv4 node contacts")
            }

            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                let contacts = bytes.chunks_exact(CONTACT_BYTES);
                if !contacts.remainder().is_empty() {
                    return Err(E::invalid_length(bytes.len(), &self));
                }
                let nodes = contacts
                    .map(|contact| {
                        let mut id = [0; 20];
                        id.copy_from_slice(&contact[..20]);
                        let mut address = [0; 6];
                        address.copy_from_slice(&contact[20..]);
                        NodeContact {
                            id: ByteArray(id),
                            address: CompactAddress::from_bytes(address).0,
                        }
                    })
                    .collect();
                Ok(CompactNodes(nodes))
            }
        }

        deserializer.deserialize_bytes(NodesVisitor)
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use serde_bencode::value::Value;

    use super::*;

    #[test]
    fn concatenates_contacts_in_network_byte_order() {
        let nodes = CompactNodes(vec![
            NodeContact {
                id: ByteArray(*b"abcdefghij0123456789"),
                address: SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 6881),
            },
            NodeContact {
                id: ByteArray([255; 20]),
                address: SocketAddrV4::new(Ipv4Addr::new(5, 6, 7, 8), 65535),
            },
        ]);
        let mut expected = b"52:abcdefghij0123456789\x01\x02\x03\x04\x1a\xe1".to_vec();
        expected.extend_from_slice(&[255; 20]);
        expected.extend_from_slice(b"\x05\x06\x07\x08\xff\xff");
        assert_eq!(serde_bencode::to_bytes(&nodes).unwrap(), expected);
        assert_eq!(
            serde_bencode::from_bytes::<CompactNodes>(&expected).unwrap(),
            nodes
        );
    }

    #[test]
    fn empty_nodes_use_empty_string() {
        assert_eq!(
            serde_bencode::to_bytes(&CompactNodes(vec![])).unwrap(),
            b"0:"
        );
        assert_eq!(
            serde_bencode::from_bytes::<CompactNodes>(b"0:").unwrap(),
            CompactNodes(vec![])
        );
    }

    #[test]
    fn rejects_partial_contacts_and_non_strings() {
        for length in [1, 20, 25, 27, 51, 53] {
            let bytes = serde_bencode::to_bytes(&Value::Bytes(vec![0; length])).unwrap();
            assert!(serde_bencode::from_bytes::<CompactNodes>(&bytes).is_err());
        }
        for value in [
            Value::Int(1),
            Value::List(vec![]),
            Value::List(vec![Value::Int(0); 26]),
        ] {
            let bytes = serde_bencode::to_bytes(&value).unwrap();
            assert!(serde_bencode::from_bytes::<CompactNodes>(&bytes).is_err());
        }
    }
}
