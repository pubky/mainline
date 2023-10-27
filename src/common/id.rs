//! Kademlia node Id or a lookup target
use rand::Rng;
use std::fmt::{self, Debug, Formatter};

use crate::{Error, Result};

/// The size of node IDs in bits.
pub const ID_SIZE: usize = 20;
pub const MAX_DISTANCE: u8 = ID_SIZE as u8 * 8;

#[derive(Clone, Copy, PartialEq, Ord, PartialOrd, Eq)]
/// Kademlia node Id or a lookup target
pub struct Id(pub [u8; ID_SIZE]);

impl Id {
    pub fn random() -> Id {
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 20] = rng.gen();

        Id(random_bytes)
    }
    /// Create a new Id from some bytes. Returns Err if `bytes` is not of length
    /// [ID_SIZE](crate::common::ID_SIZE).
    pub fn from_bytes<T: AsRef<[u8]>>(bytes: T) -> Result<Id> {
        let bytes = bytes.as_ref();
        if bytes.len() != ID_SIZE {
            return Err(Error::InvalidIdSize(bytes.len()));
        }

        let mut tmp: [u8; ID_SIZE] = [0; ID_SIZE];
        tmp[..ID_SIZE].clone_from_slice(&bytes[..ID_SIZE]);

        Ok(Id(tmp))
    }

    /// Simplified XOR distance between this Id and a target Id.
    ///
    /// The distance is the number of trailing non zero bits in the XOR result.
    ///
    /// Distance to self is 0
    /// Distance to the furthest Id is 160
    /// Distance to an Id with 5 leading matching bits is 155
    pub fn distance(&self, other: &Id) -> u8 {
        for i in 0..ID_SIZE {
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

    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl Debug for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Id({:x?})", &self.0)
    }
}
