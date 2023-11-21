//! Helper functions and structs for mutable items.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha1_smol::Sha1;
use std::convert::TryFrom;

use crate::{Error, Id, Result};

#[derive(Clone, Debug)]
pub struct MutableItem {
    /// ed25519 public key
    pub key: Vec<u8>,
    /// sequence number
    pub seq: i64,
    /// mutable value
    pub value: Vec<u8>,
    /// ed25519 signature
    pub signature: Vec<u8>,
    /// Optional salt
    pub salt: Option<Vec<u8>>,
}

pub fn target_from_key(public_key: &[u8; 32], salt: &Option<Vec<u8>>) -> Id {
    let mut encoded = vec![];

    encoded.extend(public_key.to_vec());

    if let Some(salt) = salt {
        encoded.extend(salt);
    }

    let mut hasher = Sha1::new();
    hasher.update(&encoded);
    let hash = hasher.digest().bytes();

    Id::from_bytes(hash).unwrap()
}

pub fn verify_mutable_raw(item: &MutableItem) -> Result<()> {
    let signable = encode_signable(item);

    let public_key =
        VerifyingKey::try_from(item.key.as_slice()).map_err(|_| Error::InvalidMutablePublicKey)?;
    let signature =
        Signature::from_slice(&item.signature).map_err(|_| Error::InvalidMutableSignature)?;

    let signable = encode_signable(item);

    verify_mutable(&public_key, &signature, &signable)?;

    Ok(())
}

pub fn verify_mutable(
    public_key: &VerifyingKey,
    signature: &Signature,
    signable: &[u8],
) -> Result<()> {
    public_key
        .verify(signable, signature)
        .map_err(|_| Error::InvalidMutableSignature)?;

    Ok(())
}

pub fn encode_signable(item: &MutableItem) -> Vec<u8> {
    let mut signable = vec![];

    if let Some(salt) = &item.salt {
        signable.extend(format!("4:salt{}:", salt.len()).into_bytes());
        signable.extend(salt);
    }

    signable.extend(format!("3:seqi{}e1:v{}:", item.seq, item.value.len()).into_bytes());
    signable.extend(item.value.clone());

    signable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signable_without_salt() {
        let item = MutableItem {
            key: vec![0; 32],
            seq: 4,
            value: b"Hello world!".to_vec(),
            signature: vec![0; 64],
            salt: None,
        };

        let signable = encode_signable(&item);

        assert_eq!(signable, b"3:seqi4e1:v12:Hello world!");
    }
    #[test]
    fn signable_with_salt() {
        let item = MutableItem {
            key: vec![0; 32],
            seq: 4,
            value: b"Hello world!".to_vec(),
            signature: vec![0; 64],
            salt: Some(b"foobar".to_vec()),
        };

        let signable = encode_signable(&item);

        assert_eq!(signable, b"4:salt6:foobar3:seqi4e1:v12:Hello world!");
    }
}
