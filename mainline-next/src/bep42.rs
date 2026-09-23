//! IPv4 BEP 42 node-contact generation and validation.

use std::{
    fmt,
    net::{Ipv4Addr, SocketAddrV4},
};

use crc::{Crc, CRC_32_ISCSI};

use crate::wire::{ByteArray, Id, NodeContact, ID_BYTES};

/// IPv4 address bits included in the BEP 42 CRC32C input.
const BEP42_IPV4_MASK: u32 = 0x030f_3fff;
/// Number of low bits of the ID's last byte used as BEP 42's random value `r`.
const BEP42_RANDOM_BITS: u32 = 3;
/// Selects the three bits of `r` from the ID's last byte.
const BEP42_RANDOM_MASK: u8 = 0b0000_0111;
/// The high five bits of the third ID byte constrained by the BEP 42 prefix.
const BEP42_PREFIX_MASK: u8 = !BEP42_RANDOM_MASK;
/// The Castagnoli CRC used to derive the BEP 42 prefix.
const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// A node contact accepted by the IPv4 BEP 42 validation policy.
///
/// Private, link-local, and loopback addresses are exempt from the ID check.
/// Acceptance does not establish reachability, address eligibility, or trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bep42NodeContact(NodeContact);

impl Bep42NodeContact {
    /// Generate a BEP 42 contact for a known external IPv4 socket address.
    ///
    /// The caller supplies the address; this method performs no address
    /// discovery or reachability check.
    ///
    /// # Errors
    ///
    /// Returns the OS randomness error if entropy cannot be obtained.
    pub(crate) fn generate(address: SocketAddrV4) -> Result<Self, getrandom::Error> {
        let mut random_id = [0; ID_BYTES];
        getrandom::fill(&mut random_id)?;
        Ok(Self::from_random_bytes(address, random_id))
    }

    fn from_random_bytes(address: SocketAddrV4, mut random_id: [u8; ID_BYTES]) -> Self {
        let prefix = id_prefix(*address.ip(), random_id[ID_BYTES - 1]);

        random_id[0] = prefix[0];
        random_id[1] = prefix[1];
        random_id[2] = prefix[2] | (random_id[2] & BEP42_RANDOM_MASK);

        Self(NodeContact {
            id: ByteArray(random_id),
            address,
        })
    }
}

impl TryFrom<NodeContact> for Bep42NodeContact {
    type Error = NonCompliantNodeId;

    fn try_from(contact: NodeContact) -> Result<Self, Self::Error> {
        let ip = *contact.address.ip();
        if is_exempt(ip) || id_matches_ipv4(&contact.id, ip) {
            Ok(Self(contact))
        } else {
            Err(NonCompliantNodeId { contact })
        }
    }
}

/// A node contact whose ID does not satisfy BEP 42 for its IPv4 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NonCompliantNodeId {
    contact: NodeContact,
}

impl NonCompliantNodeId {
    pub(crate) fn into_contact(self) -> NodeContact {
        self.contact
    }
}

impl fmt::Display for NonCompliantNodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("node ID is not valid for its IPv4 address")
    }
}

impl std::error::Error for NonCompliantNodeId {}

fn id_matches_ipv4(id: &Id, ip: Ipv4Addr) -> bool {
    let prefix = id_prefix(ip, id.0[ID_BYTES - 1]);

    id.0[0] == prefix[0] && id.0[1] == prefix[1] && id.0[2] & BEP42_PREFIX_MASK == prefix[2]
}

/// Return the 21-bit BEP 42 prefix with the unused low three bits cleared.
fn id_prefix(ip: Ipv4Addr, random: u8) -> [u8; 3] {
    let masked_ip = u32::from(ip) & BEP42_IPV4_MASK;
    let random_bits = u32::from(random & BEP42_RANDOM_MASK);
    let shifted_random_bits = random_bits << (u32::BITS - BEP42_RANDOM_BITS);
    let crc_input = masked_ip | shifted_random_bits;
    let [first, second, third, _fourth] = CRC32C.checksum(&crc_input.to_be_bytes()).to_be_bytes();

    [first, second, third & BEP42_PREFIX_MASK]
}

fn is_exempt(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local() || ip.is_loopback()
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use hex_literal::hex;

    use super::*;

    const PORT: u16 = 6881;
    const OFFICIAL_VECTORS: [Bep42TestVector; 5] = [
        Bep42TestVector {
            ip: "124.31.75.21",
            id: hex!("5fbfbff10c5d6a4ec8a88e4c6ab4c28b95eee401"),
        },
        Bep42TestVector {
            ip: "21.75.31.124",
            id: hex!("5a3ce9c14e7a08645677bbd1cfe7d8f956d53256"),
        },
        Bep42TestVector {
            ip: "65.23.51.170",
            id: hex!("a5d43220bc8f112a3d426c84764f8c2a1150e616"),
        },
        Bep42TestVector {
            ip: "84.124.73.14",
            id: hex!("1b0321dd1bb1fe518101ceef99462b947a01ff41"),
        },
        Bep42TestVector {
            ip: "43.213.53.83",
            id: hex!("e56f6cbf5b7c4be0237986d5243b87aa6d51305a"),
        },
    ];

    #[derive(Clone, Copy)]
    struct Bep42TestVector {
        ip: &'static str,
        id: [u8; ID_BYTES],
    }

    fn ipv4(ip: &str) -> Ipv4Addr {
        ip.parse().expect("test address should be valid IPv4")
    }

    fn socket_address(ip: &str) -> SocketAddrV4 {
        SocketAddrV4::new(ipv4(ip), PORT)
    }

    fn contact(ip: &str, id: [u8; ID_BYTES]) -> NodeContact {
        NodeContact {
            id: ByteArray(id),
            address: socket_address(ip),
        }
    }

    #[test]
    fn accepts_official_bep_42_ipv4_vectors() {
        for vector in OFFICIAL_VECTORS {
            let contact = contact(vector.ip, vector.id);
            let validated = Bep42NodeContact::try_from(contact).unwrap();
            assert_eq!(validated.0, contact);
        }
    }

    #[test]
    fn port_changes_preserve_validity_and_the_supplied_address() {
        for vector in OFFICIAL_VECTORS {
            for port in [0, 1, PORT, u16::MAX] {
                let mut contact = contact(vector.ip, vector.id);
                contact.address.set_port(port);

                let validated = Bep42NodeContact::try_from(contact).unwrap();

                assert_eq!(validated.0, contact);
            }
        }
    }

    #[test]
    fn rejects_a_contact_after_a_masked_ip_bit_changes() {
        for vector in OFFICIAL_VECTORS {
            let mut contact = contact(vector.ip, vector.id);
            assert!(Bep42NodeContact::try_from(contact).is_ok());

            // The least significant address bit participates in the CRC.
            let altered_ip = Ipv4Addr::from(u32::from(*contact.address.ip()) ^ 1);
            contact.address.set_ip(altered_ip);

            let error = Bep42NodeContact::try_from(contact).unwrap_err();

            assert_eq!(error.into_contact(), contact);
        }
    }

    #[test]
    fn rejects_a_noncompliant_id() {
        let vector = OFFICIAL_VECTORS[0];
        let mut contact = contact(vector.ip, vector.id);
        contact.id.0[0] ^= 1;

        let error = Bep42NodeContact::try_from(contact).unwrap_err();

        assert_eq!(error.into_contact(), contact);
    }

    #[test]
    fn validation_checks_exactly_21_prefix_bits() {
        let vector = OFFICIAL_VECTORS[0];
        let contact = contact(vector.ip, vector.id);

        // The last byte is tested separately because its low three bits select
        // the CRC prefix instead of forming part of the constrained prefix.
        for bit in 0..(ID_BYTES - 1) * u8::BITS as usize {
            let mut altered = contact;
            altered.id.0[bit / 8] ^= 0x80 >> (bit % 8);
            assert_eq!(
                Bep42NodeContact::try_from(altered).is_ok(),
                bit >= 21,
                "unexpected validation result after changing ID bit {bit}"
            );
        }
    }

    #[test]
    fn validation_uses_only_the_low_three_bits_of_the_random_byte() {
        let vector = OFFICIAL_VECTORS[0];
        let contact = contact(vector.ip, vector.id);

        for bit in 0..u8::BITS {
            let mut altered = contact;
            altered.id.0[ID_BYTES - 1] ^= 1 << bit;
            assert_eq!(
                Bep42NodeContact::try_from(altered).is_ok(),
                bit >= BEP42_RANDOM_BITS,
                "unexpected validation result after changing random-byte bit {bit}"
            );
        }
    }

    #[test]
    fn generation_matches_official_bep_42_vectors() {
        for vector in OFFICIAL_VECTORS {
            let address = socket_address(vector.ip);
            let mut random = vector.id;
            random[0] = 0;
            random[1] = 0;
            random[2] &= BEP42_RANDOM_MASK;

            let generated = Bep42NodeContact::from_random_bytes(address, random);

            assert_eq!(
                generated.0,
                NodeContact {
                    id: ByteArray(vector.id),
                    address,
                }
            );
        }
    }

    #[test]
    fn generation_preserves_unconstrained_bits_and_random_byte() {
        let address = socket_address(OFFICIAL_VECTORS[0].ip);

        for random_byte in 0..=u8::MAX {
            let mut random = [0xa5; ID_BYTES];
            random[ID_BYTES - 1] = random_byte;

            let generated = Bep42NodeContact::from_random_bytes(address, random);

            assert_eq!(
                generated.0.id.0[2] & BEP42_RANDOM_MASK,
                0xa5 & BEP42_RANDOM_MASK
            );
            assert_eq!(&generated.0.id.0[3..], &random[3..]);
            assert!(id_matches_ipv4(&generated.0.id, *address.ip()));
        }
    }

    #[test]
    fn os_random_generation_produces_a_valid_contact() {
        let address = socket_address(OFFICIAL_VECTORS[0].ip);
        let generated = Bep42NodeContact::generate(address).unwrap();

        assert!(id_matches_ipv4(&generated.0.id, *address.ip()));
        assert_eq!(generated.0.address, address);
    }

    #[test]
    fn prefix_uses_only_the_low_three_bits_of_the_random_byte() {
        for vector in OFFICIAL_VECTORS {
            let ip = ipv4(vector.ip);
            let random = vector.id[ID_BYTES - 1];
            let prefix = id_prefix(ip, random);

            assert_eq!(id_prefix(ip, random ^ 0b1111_1000), prefix);
            assert_ne!(id_prefix(ip, random ^ 0b0000_0001), prefix);
        }
    }

    #[test]
    fn prefix_uses_only_the_masked_ipv4_bits() {
        for vector in OFFICIAL_VECTORS {
            let ip = ipv4(vector.ip);
            let random = vector.id[ID_BYTES - 1];
            let prefix = id_prefix(ip, random);

            for bit in 0..u32::BITS {
                let altered_ip = Ipv4Addr::from(u32::from(ip) ^ (1 << bit));
                // BEP 42 retains 2, 4, 6, and 8 low bits of successive octets.
                let is_masked_bit = matches!(bit, 0..=13 | 16..=19 | 24..=25);
                assert_eq!(
                    id_prefix(altered_ip, random) != prefix,
                    is_masked_bit,
                    "unexpected prefix after changing address bit {bit} of {ip}"
                );
            }
        }
    }

    #[test]
    fn accepts_arbitrary_ids_for_exempt_ipv4_ranges() {
        for ip in [
            "10.0.0.0",
            "10.255.255.255",
            "172.16.0.0",
            "172.31.255.255",
            "192.168.0.0",
            "192.168.255.255",
            "169.254.0.0",
            "169.254.255.255",
            "127.0.0.0",
            "127.255.255.255",
        ] {
            assert!(
                Bep42NodeContact::try_from(contact(ip, [0; ID_BYTES])).is_ok(),
                "{ip} should be exempt"
            );
        }
    }

    #[test]
    fn validates_addresses_outside_the_exempt_ranges() {
        for ip in [
            "9.255.255.255",
            "11.0.0.0",
            "172.15.255.255",
            "172.32.0.0",
            "192.167.255.255",
            "192.169.0.0",
            "169.253.255.255",
            "169.255.0.0",
            "126.255.255.255",
            "128.0.0.0",
            "0.0.0.0",
            "100.64.0.1",
            "224.0.0.1",
        ] {
            assert!(
                Bep42NodeContact::try_from(contact(ip, [0; ID_BYTES])).is_err(),
                "{ip} should require BEP 42 validation"
            );
        }
    }
}
