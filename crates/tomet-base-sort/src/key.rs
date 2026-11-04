//! Opaque collation keys for two-level lexicographical ordering.

/// Packs a collation slot index and a scalar value into a 64-bit primary weight.
#[inline]
pub(crate) const fn pack_primary(slot: u32, scalar: u32) -> u64 {
    ((slot as u64) << 32) | (scalar as u64)
}

/// A 2-level collation key used for comparing strings.
///
/// Collation keys are compared lexicographically by primary weights first.
/// If and only if both strings produce identical primary weight sequences,
/// ties are broken by secondary weights.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SortKey {
    pub(crate) primary: Vec<u64>,
    pub(crate) secondary: Vec<u32>,
}
