//! Pure in-memory search and query engine for Tomet documents.
//!
//! Provides:
//! - **[`structural`]**: AST element query matching by tag, key, and value content.
//! - **[`filter`]**: Document metadata query filtering and sorting over in-memory rows.

pub mod filter;
pub mod structural;

pub use filter::{
    IndexQueryError, IndexRow, Query as FilterQuery, SortKey, is_index_document, known_paths,
};
pub use structural::{
    StructuralMatch, StructuralQuery, count_structural_matches, find_structural_matches,
    matches_query,
};
