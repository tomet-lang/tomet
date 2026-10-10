//! =[ elements ]
//!
//! Semantic interpretations and normalization helpers for concrete built-in elements.
//!
//! While the root of `tomet-semantics` provides the general semantic framework
//! (element classification, vocabulary schemas, positional argument normalization,
//! and connect evaluation), this module houses the logic specific to individual
//! built-in element families:
//!
//! - (config) Document configuration (`@config`, `@settings`).
//! - (embedded) An element's `{...}` value read as data.
//! - (footnote) Footnote collection and indexing (`<footnote>`, `^id`).
//! - (heading) Heading level calculation and bounds clamping (`heading`).
//! - (meta) Document metadata and frontmatter extraction (`@meta`, `@kind`, `@version`).
//! - (table) Table rows and cell structures (`@table`).
//! - (tag) Tag collection (`#tag`).
//! - (target) Target URI and scheme resolution (`@link`, `@embed`, `@file`, `@dir`).

pub mod config;
pub mod embedded;
pub mod footnote;
pub mod heading;
pub mod meta;
pub mod table;
pub mod tag;
pub mod target;
