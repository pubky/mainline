//! Helper functions and structs for mutable items.

use bytes::Bytes;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha1_smol::Sha1;
use std::convert::TryFrom;

use crate::{Error, Id, Result};

#[derive(Clone, Debug, PartialEq)]
/// [Bep_0044](https://www.bittorrent.org/beps/bep_0044.html)'s Mutable item.
pub struct MutableItem {
    /// hash of the key and optional salt
    target: Id,
    /// ed25519 public key
    key: [u8; 32],
    /// sequence number
    seq: i64,
    /// mutable value
    value: Bytes,
    /// ed25519 signature
    signature: [u8; 64],
    /// Optional salt
    salt: Option<Bytes>,
    /// Optional compare and swap seq
    cas: Option<i64>,
}

impl MutableItem {
    /// Create a new mutable item from a signing key, value, sequence number and optional salt.
    pub fn new(signer: SigningKey, value: Bytes, seq: i64, salt: Option<Bytes>) -> Self {
        let signable = encode_signable(&seq, &value, &salt);
        let signature = signer.sign(&signable);

        Self::new_signed_unchecked(
            signer.verifying_key().to_bytes(),
            signature.into(),
            value,
            seq,
            salt,
        )
    }

    /// Return the target of a [MutableItem] by hashing its `public_key` and an optional `salt`
    pub fn target_from_key(public_key: &[u8; 32], salt: &Option<Bytes>) -> Id {
        let mut encoded = vec![];

        encoded.extend(public_key);

        if let Some(salt) = salt {
            encoded.extend(salt);
        }

        let mut hasher = Sha1::new();
        hasher.update(&encoded);
        let bytes = hasher.digest().bytes();

        Id { bytes }
    }

    /// Set the cas number if needed.
    pub fn with_cas(mut self, cas: i64) -> Self {
        self.cas = Some(cas);
        self
    }

    /// Create a new mutable item from an already signed value.
    pub fn new_signed_unchecked(
        key: [u8; 32],
        signature: [u8; 64],
        value: Bytes,
        seq: i64,
        salt: Option<Bytes>,
    ) -> Self {
        Self {
            target: MutableItem::target_from_key(&key, &salt),
            key,
            value,
            seq,
            signature,
            salt,
            cas: None,
        }
    }

    pub(crate) fn from_dht_message(
        target: &Id,
        key: &[u8],
        v: Bytes,
        seq: &i64,
        signature: &[u8],
        salt: Option<Bytes>,
        cas: &Option<i64>,
    ) -> Result<Self> {
        let key = VerifyingKey::try_from(key).map_err(|_| Error::InvalidMutablePublicKey)?;

        let signature =
            Signature::from_slice(signature).map_err(|_| Error::InvalidMutableSignature)?;

        key.verify(&encode_signable(seq, &v, &salt), &signature)
            .map_err(|_| Error::InvalidMutableSignature)?;

        Ok(Self {
            target: *target,
            key: key.to_bytes(),
            value: v,
            seq: *seq,
            signature: signature.to_bytes(),
            salt: salt.to_owned(),
            cas: *cas,
        })
    }

    // === Getters ===

    pub fn target(&self) -> &Id {
        &self.target
    }

    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }

    pub fn value(&self) -> &Bytes {
        &self.value
    }

    pub fn seq(&self) -> &i64 {
        &self.seq
    }

    pub fn signature(&self) -> &[u8; 64] {
        &self.signature
    }

    pub fn salt(&self) -> &Option<Bytes> {
        &self.salt
    }

    pub fn cas(&self) -> &Option<i64> {
        &self.cas
    }
}

pub fn encode_signable(seq: &i64, value: &Bytes, salt: &Option<Bytes>) -> Bytes {
    let mut signable = vec![];

    if let Some(salt) = salt {
        signable.extend(format!("4:salt{}:", salt.len()).into_bytes());
        signable.extend(salt);
    }

    signable.extend(format!("3:seqi{}e1:v{}:", seq, value.len()).into_bytes());
    signable.extend(value);

    signable.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signable_without_salt() {
        let signable = encode_signable(&4, &Bytes::from_static(b"Hello world!"), &None);

        assert_eq!(&*signable, b"3:seqi4e1:v12:Hello world!");
    }
    #[test]
    fn signable_with_salt() {
        let signable = encode_signable(
            &4,
            &Bytes::from_static(b"Hello world!"),
            &Some(Bytes::from_static(b"foobar")),
        );

        assert_eq!(&*signable, b"4:salt6:foobar3:seqi4e1:v12:Hello world!");
    }
}
