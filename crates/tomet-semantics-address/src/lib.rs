//! =[ tomet-address ]
//!
//! What a link or an element points at, recognized without ever touching
//! disk: the scheme of a link target ([`Address`], [`is_explicit_scheme`]),
//! and whether an anchor exists in a document ([`has_anchor`]).
//!
//! An expression's `#id` and `doc#id` are not resolved here any more: that
//! is the evaluator's job (`tomet-expr`), which answers it through a
//! resolver so that it can reach other documents.

mod address;
mod anchor;

pub use address::{Address, AddressKind, EXPLICIT_SCHEMES, LinkKind, is_explicit_scheme};
pub use anchor::{collect_anchors, find_anchor, has_anchor};
