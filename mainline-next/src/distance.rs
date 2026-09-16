use crate::wire::{Id, ID_BYTES};

const BITS_IN_BYTE: usize = u8::BITS as usize;
const ID_BITS: usize = ID_BYTES * BITS_IN_BYTE;

/// An XOR distance between two IDs, ordered as a big-endian integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Distance([u8; ID_BYTES]);

#[allow(dead_code)]
impl Id {
    /// Return the full XOR distance between two IDs.
    pub(crate) fn xor_distance(&self, other: &Self) -> Distance {
        Distance(std::array::from_fn(|index| self.0[index] ^ other.0[index]))
    }

    /// Return the Kademlia bucket containing `other` relative to `self`.
    ///
    /// Equal IDs have distance zero. For non-equal IDs, the result is the bit
    /// length of the XOR distance.
    pub(crate) fn bucket_distance(&self, other: &Self) -> u8 {
        for (index, (&left, &right)) in self.0.iter().zip(&other.0).enumerate() {
            let byte = left ^ right;
            if byte != 0 {
                return (ID_BITS - index * BITS_IN_BYTE - byte.leading_zeros() as usize) as u8;
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{Distance, ID_BITS, ID_BYTES};
    use crate::wire::{ByteArray, Id};
    use hex_literal::hex;

    #[test]
    fn xor_distance_is_symmetric_and_zero_for_equal_ids() {
        let left: Id = ByteArray(hex!("6162636465666768696a30313233343536373839"));
        let right: Id = ByteArray(hex!("303132333435363738396162636465666768696a"));

        assert_eq!(left.xor_distance(&left), Distance([0; ID_BYTES]));
        assert_eq!(left.xor_distance(&right), right.xor_distance(&left));
    }

    #[test]
    fn xor_distance_matches_full_width_regression_vector() {
        let left: Id = ByteArray(hex!("0639a1e24fbb8ab277df033476ab0de10fab3bdc"));
        let right: Id = ByteArray(hex!("035b1aeb9737ade1a80933594f405d3f772aa08e"));
        let expected = Distance(hex!("0562bb09d88c2753dfd6306d39eb50de78819b52"));

        assert_eq!(left.xor_distance(&right), expected);
    }

    #[test]
    fn xor_distance_matches_k_bucket_vectors() {
        // Adapted to 20-byte IDs from:
        // https://github.com/tristanls/k-bucket/blob/fff76ece282a65bba0ab82e11e1ad46d440d9048/test/defaultDistance.js
        let vectors = [
            (
                hex!("0000000000000000000000000000000000000000"),
                hex!("0000000000000000000000000000000000000000"),
                hex!("0000000000000000000000000000000000000000"),
            ),
            (
                hex!("0000000000000000000000000000000000000000"),
                hex!("0000000000000000000000000000000000000001"),
                hex!("0000000000000000000000000000000000000001"),
            ),
            (
                hex!("0000000000000000000000000000000000000002"),
                hex!("0000000000000000000000000000000000000001"),
                hex!("0000000000000000000000000000000000000003"),
            ),
            (
                hex!("0000000000000000000000000000000000000124"),
                hex!("0000000000000000000000000000000000004024"),
                hex!("0000000000000000000000000000000000004100"),
            ),
        ];

        for (left, right, expected) in vectors {
            let left: Id = ByteArray(left);
            let right: Id = ByteArray(right);
            assert_eq!(left.xor_distance(&right), Distance(expected));
        }
    }

    #[test]
    fn xor_distances_are_ordered_as_big_endian_values() {
        let target: Id = ByteArray(hex!("0000000000000000000000000000000000000000"));
        let low: Id = ByteArray(hex!("00000000000000000000000000000000000000ff"));
        let high: Id = ByteArray(hex!("0100000000000000000000000000000000000000"));

        assert!(low.xor_distance(&target) < high.xor_distance(&target));
    }

    #[test]
    fn xor_distance_orders_ids_within_one_bucket() {
        let target: Id = ByteArray(hex!("ffffffffffffffffffffffffffffffffffffffff"));
        let near: Id = ByteArray(hex!("fffffffffffffffffffffffffffffffffffffffc"));
        let closer: Id = ByteArray(hex!("fffffffffffffffffffffffffffffffffffffffd"));

        assert_eq!(
            near.bucket_distance(&target),
            closer.bucket_distance(&target)
        );
        assert!(closer.xor_distance(&target) < near.xor_distance(&target));
    }

    #[test]
    fn bucket_distance_matches_go_libp2p_prefix_vectors() {
        // Adapted to 20-byte IDs from:
        // https://github.com/libp2p/go-libp2p-kbucket/blob/4cdb61dd32115753f29ce405199945d1140a25be/keyspace/xor_test.go
        let target: Id = ByteArray(hex!("0000000000000000000000000000000000000000"));

        let distance = ByteArray(hex!("0000008000000000000000000000000000000000"));
        assert_eq!(target.bucket_distance(&distance), 136);

        let distance = ByteArray(hex!("0058ff800000f000000000000000000000000000"));
        assert_eq!(target.bucket_distance(&distance), 151);
    }

    #[test]
    fn bucket_distance_is_the_xor_bit_length() {
        let target: Id = ByteArray(hex!("a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5"));

        assert_eq!(target.bucket_distance(&target), 0);
        for bit in 0..ID_BITS {
            let mut bytes = target.0;
            bytes[bit / 8] ^= 0x80 >> (bit % 8);
            assert_eq!(
                usize::from(target.bucket_distance(&ByteArray(bytes))),
                ID_BITS - bit
            );
        }
    }
}
