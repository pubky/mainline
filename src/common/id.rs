//! Kademlia node Id or a lookup target
use crc::{Crc, CRC_32_ISCSI};
use rand::Rng;
use std::convert::TryInto;
use std::{
    fmt::{self, Debug, Display, Formatter},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use crate::{Error, Result};

/// The size of node IDs in bits.
pub const ID_SIZE: usize = 20;
pub const MAX_DISTANCE: u8 = ID_SIZE as u8 * 8;

const IPV4_MASK: u32 = 0x030f3fff;
const CASTAGNOLI: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

#[derive(Clone, Copy, PartialEq, Ord, PartialOrd, Eq, Hash)]
/// Kademlia node Id or a lookup target
pub struct Id {
    pub bytes: [u8; ID_SIZE],
}

impl Id {
    pub fn random() -> Id {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 20] = rng.gen();

        Id { bytes }
    }
    /// Create a new Id from some bytes. Returns Err if the input is not 20 bytes long.
    pub fn from_bytes<T: AsRef<[u8]>>(bytes: T) -> Result<Id> {
        let bytes = bytes.as_ref();
        if bytes.len() != ID_SIZE {
            return Err(Error::InvalidIdSize(bytes.len()));
        }

        let mut tmp: [u8; ID_SIZE] = [0; ID_SIZE];
        tmp[..ID_SIZE].clone_from_slice(&bytes[..ID_SIZE]);

        Ok(Id { bytes: tmp })
    }

    /// Simplified XOR distance between this Id and a target Id.
    ///
    /// The distance is the number of trailing non zero bits in the XOR result.
    ///
    /// Distance to self is 0
    /// Distance to the furthest Id is 160
    /// Distance to an Id with 5 leading matching bits is 155
    pub fn distance(&self, other: &Id) -> u8 {
        for (i, (a, b)) in self.bytes.iter().zip(other.bytes).enumerate() {
            if a != &b {
                // leading zeros so far + laedinge zeros of this byte
                let leading_zeros = (i as u32 * 8 + (a ^ b).leading_zeros()) as u8;

                return MAX_DISTANCE - leading_zeros;
            }
        }

        0
    }

    pub fn to_vec(self) -> Vec<u8> {
        self.bytes.to_vec()
    }

    /// Create a new Id according to [BEP0042](http://bittorrent.org/beps/bep_0042.html).
    pub fn from_addr(addr: &SocketAddr) -> Id {
        let ip = addr.ip();

        let mut rng = rand::thread_rng();
        let r: u8 = rng.gen();

        let bytes: [u8; 20] = rng.gen();

        match ip {
            IpAddr::V4(addr) => from_ipv4_and_r(bytes, addr, r),
            IpAddr::V6(_addr) => unimplemented!(),
        }
    }

    /// Validate that this Id is valid with respect to [BEP0042](http://bittorrent.org/beps/bep_0042.html).
    pub fn is_valid_for_ip(&self, ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => {
                if ipv4.is_private() {
                    return true;
                }

                let expected = first_21_bits(&id_prefix_ipv4(ipv4, self.bytes[ID_SIZE - 1]));
                let actual = first_21_bits(&self.bytes);

                expected == actual
            }
            IpAddr::V6(_ipv6) => {
                unimplemented!()

                // // For IPv6, checking the ULA range fc00::/7
                // if (ipv6.segments()[0] & 0xFE00 == 0xFC00) {
                //     return true;
                // }
            }
        }
    }
}

fn first_21_bits(bytes: &[u8]) -> [u8; 3] {
    [bytes[0], bytes[1], bytes[2] & 0xf8]
}

fn from_ipv4_and_r(bytes: [u8; 20], ip: Ipv4Addr, r: u8) -> Id {
    let mut bytes = bytes;
    let prefix = id_prefix_ipv4(&ip, r);

    // Set first 21 bits to the prefix
    bytes[0] = prefix[0];
    bytes[1] = prefix[1];
    // set the first 5 bits of the 3 byte to the remaining 5 bits of the prefix
    bytes[2] = (prefix[2] & 0xf8) | (bytes[2] & 0x7);

    // Set the last byte to the random r
    bytes[ID_SIZE - 1] = r;

    Id { bytes }
}

fn id_prefix_ipv4(ip: &Ipv4Addr, r: u8) -> [u8; 3] {
    let r32: u32 = r.into();
    let ip_int: u32 = u32::from_be_bytes(ip.octets());
    let nonsense: u32 = (ip_int & IPV4_MASK) | (r32 << 29);

    let mut digest = CASTAGNOLI.digest();
    digest.update(&nonsense.to_be_bytes());

    let crc = digest.finalize();

    crc.to_be_bytes()[..3]
        .try_into()
        .expect("Failed to convert bytes 0-2 of the crc into a 3-byte array")
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[allow(clippy::format_collect)]
        let hex_chars: String = self
            .bytes
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect();

        write!(f, "{}", hex_chars)
    }
}

impl From<[u8; ID_SIZE]> for Id {
    fn from(bytes: [u8; ID_SIZE]) -> Id {
        Id { bytes }
    }
}

impl FromStr for Id {
    type Err = Error;

    fn from_str(s: &str) -> Result<Id> {
        if s.len() % 2 != 0 {
            return Err(Error::InvalidIdEncoding(
                "Number of Hex characters should be even".into(),
            ));
        }

        let mut bytes = Vec::with_capacity(s.len() / 2);

        for i in 0..s.len() / 2 {
            let byte_str = &s[i * 2..(i * 2) + 2];
            if let Ok(byte) = u8::from_str_radix(byte_str, 16) {
                bytes.push(byte);
            } else {
                return Err(Error::Static("Invalid hex character")); // Invalid hex character
            }
        }

        Id::from_bytes(bytes)
    }
}

impl Debug for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn distance_to_self() {
        let id = Id::random();
        let distance = id.distance(&id);
        assert_eq!(distance, 0)
    }

    #[test]
    fn distance_to_id() {
        let id = Id::from_str("0639A1E24FBB8AB277DF033476AB0DE10FAB3BDC").unwrap();

        let target = Id::from_str("035b1aeb9737ade1a80933594f405d3f772aa08e").unwrap();

        let distance = id.distance(&target);

        assert_eq!(distance, 155)
    }

    #[test]
    fn distance_to_random_id() {
        let id = Id::random();
        let target = Id::random();

        let distance = id.distance(&target);

        assert_ne!(distance, 0)
    }

    #[test]
    fn distance_to_furthest() {
        let id = Id::random();

        let mut opposite = [0_u8; 20];
        for (i, &value) in id.bytes.iter().enumerate() {
            opposite[i] = value ^ 0xff;
        }
        let target = Id::from_bytes(opposite).unwrap();

        let distance = id.distance(&target);

        assert_eq!(distance, MAX_DISTANCE)
    }

    #[test]
    fn from_u8_20() {
        let bytes = [8; 20];

        let id: Id = bytes.into();

        assert_eq!(id.bytes, bytes);
    }

    #[test]
    fn from_ipv4() {
        let vectors = vec![
            (Ipv4Addr::new(124, 31, 75, 21), 1, [0x5f, 0xbf, 0xbf]),
            (Ipv4Addr::new(21, 75, 31, 124), 86, [0x5a, 0x3c, 0xe9]),
            (Ipv4Addr::new(65, 23, 51, 170), 22, [0xa5, 0xd4, 0x32]),
            (Ipv4Addr::new(84, 124, 73, 14), 65, [0x1b, 0x03, 0x21]),
            (Ipv4Addr::new(43, 213, 53, 83), 90, [0xe5, 0x6f, 0x6c]),
        ];

        for vector in vectors {
            test(vector.0, vector.1, vector.2);
        }

        fn test(ip: Ipv4Addr, r: u8, expected_prefix: [u8; 3]) {
            let id = Id::random();
            let result = from_ipv4_and_r(id.bytes, ip, r);
            let prefix = first_21_bits(&result.bytes);

            assert_eq!(prefix, first_21_bits(&expected_prefix));
            assert_eq!(result.bytes[ID_SIZE - 1], r);
        }
    }

    #[test]
    fn is_valid_for_ipv4() {
        let valid_vectors = vec![
            (
                Ipv4Addr::new(124, 31, 75, 21),
                "5fbfbff10c5d6a4ec8a88e4c6ab4c28b95eee401",
            ),
            (
                Ipv4Addr::new(21, 75, 31, 124),
                "5a3ce9c14e7a08645677bbd1cfe7d8f956d53256",
            ),
            (
                Ipv4Addr::new(65, 23, 51, 170),
                "a5d43220bc8f112a3d426c84764f8c2a1150e616",
            ),
            (
                Ipv4Addr::new(84, 124, 73, 14),
                "1b0321dd1bb1fe518101ceef99462b947a01ff41",
            ),
            (
                Ipv4Addr::new(43, 213, 53, 83),
                "e56f6cbf5b7c4be0237986d5243b87aa6d51305a",
            ),
        ];

        for vector in valid_vectors {
            test(vector.0, vector.1);
        }

        fn test(ip: Ipv4Addr, hex: &str) {
            let id = Id::from_str(hex).unwrap();

            assert!(id.is_valid_for_ip(&IpAddr::V4(ip)));
        }
    }
}
