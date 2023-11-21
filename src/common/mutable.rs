//! Helper functions and structs for mutable items.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha1_smol::Sha1;
use std::convert::TryFrom;

use crate::{Error, Id, Result};

#[derive(Clone, Debug)]
pub struct MutableItem {
    /// hash of the key and optional salt
    target: Id,
    /// ed25519 public key
    key: Box<VerifyingKey>,
    /// sequence number
    seq: i64,
    /// mutable value
    value: Vec<u8>,
    /// ed25519 signature
    signature: Signature,
    /// Optional salt
    salt: Option<Vec<u8>>,
}

impl MutableItem {
    pub fn new(signer: SigningKey, value: Vec<u8>, seq: i64, salt: Option<Vec<u8>>) -> Self {
        let signable = encode_signable(&seq, &value, &salt);
        let signature = signer.sign(&signable);

        Self {
            target: target_from_key(&signer.verifying_key(), &salt),
            key: signer.verifying_key().into(),
            value,
            seq,
            signature,
            salt,
        }
    }

    pub fn from_dht_message(
        target: &Id,
        key: &Vec<u8>,
        v: &[u8],
        seq: &i64,
        signature: &[u8],
        salt: &Option<Vec<u8>>,
    ) -> Result<Self> {
        let key =
            VerifyingKey::try_from(key.as_slice()).map_err(|_| Error::InvalidMutablePublicKey)?;

        let signature =
            Signature::from_slice(signature).map_err(|_| Error::InvalidMutableSignature)?;

        Ok(Self {
            target: *target,
            key: key.into(),
            value: v.to_owned(),
            seq: *seq,
            signature,
            salt: salt.clone(),
        })
    }

    /// === Getters ===

    pub fn target(&self) -> &Id {
        &self.target
    }

    pub fn key(&self) -> &VerifyingKey {
        &self.key
    }

    pub fn value(&self) -> &Vec<u8> {
        &self.value
    }

    pub fn seq(&self) -> &i64 {
        &self.seq
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn salt(&self) -> &Option<Vec<u8>> {
        &self.salt
    }
}

pub fn target_from_key(public_key: &VerifyingKey, salt: &Option<Vec<u8>>) -> Id {
    let mut encoded = vec![];

    encoded.extend(public_key.to_bytes());

    if let Some(salt) = salt {
        encoded.extend(salt);
    }

    let mut hasher = Sha1::new();
    hasher.update(&encoded);
    let hash = hasher.digest().bytes();

    Id::from_bytes(hash).unwrap()
}

pub fn encode_signable(seq: &i64, value: &Vec<u8>, salt: &Option<Vec<u8>>) -> Vec<u8> {
    let mut signable = vec![];

    if let Some(salt) = salt {
        signable.extend(format!("4:salt{}:", salt.len()).into_bytes());
        signable.extend(salt);
    }

    signable.extend(format!("3:seqi{}e1:v{}:", seq, value.len()).into_bytes());
    signable.extend(value);

    signable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signable_without_salt() {
        let signable = encode_signable(&4, &b"Hello world!".to_vec(), &None);

        assert_eq!(signable, b"3:seqi4e1:v12:Hello world!");
    }
    #[test]
    fn signable_with_salt() {
        let signable = encode_signable(&4, &b"Hello world!".to_vec(), &Some(b"foobar".to_vec()));

        assert_eq!(signable, b"4:salt6:foobar3:seqi4e1:v12:Hello world!");
    }
}
