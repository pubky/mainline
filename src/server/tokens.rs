//! Manage tokens for remote client IPs.

use crc::{Crc, CRC_32_ISCSI};
use rand::{thread_rng, Rng};
use std::{
    fmt::{self, Debug, Formatter},
    net::SocketAddr,
    time::Instant,
};

use tracing::trace;

const SECRET_SIZE: usize = 20;
const TOKEN_SIZE: usize = 4;
const CASTAGNOLI: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// Tokens generator.
///
/// Read [BEP_0005](https://www.bittorrent.org/beps/bep_0005.html) for more information.
pub struct Tokens {
    prev_secret: [u8; SECRET_SIZE],
    curr_secret: [u8; SECRET_SIZE],
    last_updated: Instant,
}

impl Debug for Tokens {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Tokens (_)")
    }
}

impl Tokens {
    pub fn new() -> Self {
        let mut rng = thread_rng();

        Tokens {
            prev_secret: rng.gen(),
            curr_secret: rng.gen(),
            last_updated: Instant::now(),
        }
    }

    // === Public Methods ===

    pub fn should_update(&self) -> bool {
        self.last_updated.elapsed() > crate::common::TOKEN_ROTATE_INTERVAL
    }

    /// Validate that the token was generated within the past 10 minutes
    pub fn validate(&mut self, address: SocketAddr, token: &Vec<u8>) -> bool {
        let prev = self.internal_generate_token(address, self.prev_secret);
        let curr = self.internal_generate_token(address, self.curr_secret);

        token == &curr || token == &prev
    }

    pub fn rotate(&mut self) {
        trace!("Rotating secrets");
        let mut rng = thread_rng();

        self.prev_secret = self.curr_secret;
        self.curr_secret = rng.gen();

        self.last_updated = Instant::now();
    }

    pub fn generate_token(&mut self, address: SocketAddr) -> [u8; 4] {
        self.internal_generate_token(address, self.curr_secret)
    }

    // === Private Methods ===

    fn internal_generate_token(
        &mut self,
        address: SocketAddr,
        secret: [u8; SECRET_SIZE],
    ) -> [u8; TOKEN_SIZE] {
        let mut digest = CASTAGNOLI.digest();

        let octets = match address.ip() {
            std::net::IpAddr::V4(v4) => v4.octets().to_vec(),
            std::net::IpAddr::V6(v6) => v6.octets().to_vec(),
        };

        digest.update(&octets);
        digest.update(&secret);

        let checksum = digest.finalize();

        checksum.to_be_bytes()
    }
}

impl Default for Tokens {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn valid_tokens() {
        let mut tokens = Tokens::new();

        let address = SocketAddr::from(([127, 0, 0, 1], 6881));
        let token = tokens.generate_token(address);

        assert!(tokens.validate(address, &token.to_vec()))
    }
}
