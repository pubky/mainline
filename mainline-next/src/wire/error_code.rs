use serde::{Deserialize, Serialize};

/// A KRPC error code. Unknown codes are preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ErrorCode(pub i64);

impl ErrorCode {
    pub(crate) const GENERIC: Self = Self(201);
    pub(crate) const SERVER: Self = Self(202);
    pub(crate) const PROTOCOL: Self = Self(203);
    pub(crate) const METHOD_UNKNOWN: Self = Self(204);

    /// The bencoded item exceeds the accepted size.
    pub(crate) const VALUE_TOO_LARGE: Self = Self(205);
    pub(crate) const INVALID_SIGNATURE: Self = Self(206);
    pub(crate) const SALT_TOO_LARGE: Self = Self(207);
    /// The supplied CAS sequence does not match the stored sequence.
    pub(crate) const CAS_MISMATCH: Self = Self(301);
    pub(crate) const SEQUENCE_TOO_LOW: Self = Self(302);
}
