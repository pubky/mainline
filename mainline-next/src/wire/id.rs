use super::ByteArray;

pub(crate) const ID_BYTES: usize = 20;

/// A 20-byte node ID, lookup target, or info hash on the wire.
pub(crate) type Id = ByteArray<ID_BYTES>;
