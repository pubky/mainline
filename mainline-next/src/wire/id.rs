use super::ByteArray;

/// A 20-byte node ID, lookup target, or info hash on the wire.
pub(crate) type Id = ByteArray<20>;
