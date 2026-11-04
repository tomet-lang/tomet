//! =[ Overview & Scope ]
//!
//! Pure character and dictionary collation, sort keys, and ordering primitives.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Pure base crate)
//!   | This crate is in the base layer (`crates/tomet-base-*`) and depends on
//!   | nothing in the workspace. It contains zero unsafe code and no external
//!   | dependencies.
//! - (Two-level collation)
//!   | Equivalence groups share a primary weight and are distinguished by
//!   | secondary weights. Streaming comparison compares primary weights first,
//!   | enforces prefix dominance, and breaks ties with secondary weights.
//! - (Greedy longest match)
//!   | Multi-character tokens are matched greedily using an internal UTF-8 trie,
//!   | falling back to Unicode scalar values for unmapped characters.

pub mod collator;
pub mod direction;
pub mod error;
pub mod key;
pub mod spec;
mod trie;

pub use collator::Collator;
pub use direction::Direction;
pub use error::CollationError;
pub use key::SortKey;
pub use spec::{CollationSpec, Step};

#[cfg(test)]
mod tests;
