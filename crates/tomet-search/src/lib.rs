//! =[ tomet-search ]
//!
//! Pure, in-memory search and query engine for Tomet documents.
//!
//! =[ Scope & Layering ]
//!
//! Sits in Layer 4 of the Tomet crate hierarchy. Performs pure, in-memory querying
//! and matching without filesystem I/O. Filesystem scanning and disk caching belong
//! to higher layers (`tomet-workspace-indexer` and `tomet-workspace` in Layer 5).
//!
//! =[ Query Engines ]
//!
//! - **[`structural`]**: AST element query matching by tag, key, and value content.
//! - **[`filter`]**: Document metadata query filtering (`contains`, `exists`, `eq`, `gt`)
//!   and sorting (`by(field, "asc"|"desc")`) over in-memory rows (the engine behind `${filter(...)}`).

pub mod filter;
pub mod structural;

pub use filter::{
    IndexQueryError, IndexRow, Query as FilterQuery, SortKey, is_index_document, known_paths,
};
pub use structural::{
    StructuralMatch, StructuralQuery, count_structural_matches, find_structural_matches,
    matches_query,
};
