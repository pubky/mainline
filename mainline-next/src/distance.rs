use crate::wire::{Id, ID_BYTES};

const BITS_IN_BYTE: usize = u8::BITS as usize;
const ID_BITS: usize = ID_BYTES * BITS_IN_BYTE;

#[allow(dead_code)]
impl Id {
    /// Return the full XOR distance between two IDs.
    pub(crate) fn xor_distance(&self, other: &Self) -> Self {
        Self(std::array::from_fn(|index| self.0[index] ^ other.0[index]))
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
    use super::ID_BITS;
    use crate::wire::{ByteArray, Id};
    use hex_literal::hex;

    #[test]
    fn xor_distance_is_symmetric_and_zero_for_equal_ids() {
        let left: Id = ByteArray(*b"abcdefghij0123456789");
        let right: Id = ByteArray(*b"0123456789abcdefghij");

        assert_eq!(left.xor_distance(&left), ByteArray([0; 20]));
        assert_eq!(left.xor_distance(&right), right.xor_distance(&left));
    }

    #[test]
    fn xor_distance_matches_known_vector() {
        let left: Id = ByteArray(hex!("0639a1e24fbb8ab277df033476ab0de10fab3bdc"));
        let right: Id = ByteArray(hex!("035b1aeb9737ade1a80933594f405d3f772aa08e"));
        let expected: Id = ByteArray(hex!("0562bb09d88c2753dfd6306d39eb50de78819b52"));

        assert_eq!(left.xor_distance(&right), expected);
        assert_eq!(right.xor_distance(&left), expected);
    }

    #[test]
    fn xor_distances_are_ordered_as_big_endian_values() {
        let target: Id = ByteArray([0; 20]);
        let low: Id = ByteArray([0x7f; 20]);
        let high: Id = ByteArray([0x80; 20]);

        assert!(low.xor_distance(&target) < high.xor_distance(&target));
    }

    #[test]
    fn xor_distance_orders_ids_within_one_bucket() {
        let target: Id = ByteArray([0xff; 20]);
        let mut near_bytes = [0xff; 20];
        near_bytes[19] = 0xfc;
        let near = ByteArray(near_bytes);
        near_bytes[19] = 0xfd;
        let closer = ByteArray(near_bytes);

        assert_eq!(
            near.bucket_distance(&target),
            closer.bucket_distance(&target)
        );
        assert!(closer.xor_distance(&target) < near.xor_distance(&target));
    }

    #[test]
    fn bucket_distance_is_the_xor_bit_length() {
        let target: Id = ByteArray([0xa5; 20]);

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
