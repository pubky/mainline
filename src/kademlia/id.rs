//! Kademlia node Id or a lookup target
use rand::Rng;
use std::{
    cmp::Ordering,
    fmt::{self, Debug, Formatter},
};

/// The size of node IDs in bits.
pub const ID_LENGTH: usize = 20;
pub const MAX_DISTANCE: u8 = ID_LENGTH as u8 * 8;

#[derive(Clone, Copy, PartialEq)]
/// Kademlia node Id or a lookup target
pub struct Id(pub [u8; 20]);

impl Id {
    pub fn random() -> Id {
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 20] = rng.gen();

        Id(random_bytes)
    }

    /// Simplified XOR distance between this Id and a target Id.
    ///
    /// The distance is the number of trailing non zero bits in the XOR result.
    ///
    /// Distance to self is 0
    /// Distance to the furthest Id is 160
    /// Distance to an Id with 5 leading matching bits is 155
    pub fn distance(&self, other: &Id) -> u8 {
        for i in 0..ID_LENGTH {
            let a = self.0[i];
            let b = other.0[i];

            if a != b {
                // leading zeros so far + laedinge zeros of this byte
                let leading_zeros = (i as u32 * 8 + (a ^ b).leading_zeros()) as u8;

                return MAX_DISTANCE - leading_zeros;
            }
        }

        0
    }

    pub fn cmp(&self, id: &Id) -> Ordering {
        self.0.cmp(&id.0)
    }
}

impl Debug for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Id({:x?})", &self.0)
    }
}
