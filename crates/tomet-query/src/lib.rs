//! =[ tomet-query ]
//!
//! Pure, in-memory deterministic query engine for Tomet documents.
//!
//! =[ Scope & Layering ]
//!
//! Performs pure, in-memory querying over caller-supplied documents and value
//! collections without filesystem I/O, expression evaluation, fuzzy matching,
//! or hole evaluation. Filesystem scanning and disk caching belong to higher
//! layers (`tomet-workspace-indexer` and `tomet-workspace`).
//!
//! =[ Query Engines ]
//!
//! - (`structural`) AST element query matching by tag, key, and value content.
//! - (`part`) Deterministic AST part lookup by `#id`.
//! - (`collection`) Pure filtering and key-based sorting over sequences.

pub mod collection;
pub mod part;
pub mod structural;

pub use collection::{filter_items, sort_items_by_key};
pub use part::find_by_id_with;
pub use structural::{
    StructuralMatch, StructuralQuery, count_structural_matches, find_structural_matches,
    matches_query,
};
