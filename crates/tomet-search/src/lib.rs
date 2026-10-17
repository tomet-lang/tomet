//! =[ tomet-search ]
//!
//! Pure, in-memory search and query engine for Tomet documents.
//!
//! =[ Scope & Layering ]
//!
//! Performs pure, in-memory querying and matching without filesystem I/O.
//! Filesystem scanning and disk caching belong to higher layers
//! (`tomet-workspace-indexer` and `tomet-workspace`).
//!
//! =[ Query Engines ]
//!
//! - (`structural`) AST element query matching by tag, key, and value content.
//!
//! Querying documents by their metadata is not done here: it is an expression
//! (`vault.docs.filter(d => ...)`), evaluated by `tomet-compute`.

pub mod structural;

pub use structural::{
    StructuralMatch, StructuralQuery, count_structural_matches, find_structural_matches,
    matches_query,
};
