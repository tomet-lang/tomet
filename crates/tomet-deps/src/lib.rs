//! =[ tomet-deps ]
//!
//! Document-level dependency tracking, impact analysis, and incremental invalidation
//! for Tomet workspaces and static site generators.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Vault-Wide Dependency Graph & Invalidation Engine)
//!   | `tomet-deps` models document interactions across a vault as a bidirectional directed graph.
//!   | It tracks outgoing hypertext links, transclusions/embeds, and metadata dependencies.
//! - (Fine-Grained Invalidation)
//!   | Rather than treating all file modifications equally, `tomet-deps` tracks signature differences:
//!   | - Text-only modification: re-renders only the edited document.
//!   | - Link addition/removal: re-renders the document and updates the incoming backlinks of the target.
//!   | - Title/export changes: propagates re-render requirements to all documents referencing the target.
//!   | - Transclusion / Embed: propagates content changes recursively to all embedding documents.
//! - (Pure In-Memory & Platform Agnostic)
//!   | Zero filesystem I/O. All models and graph algorithms are pure in-memory computations,
//!   | making `tomet-deps` fully compatible with native CLI, background watch runners, and Wasm targets.

pub mod extract;
pub mod graph;
pub mod model;

pub use extract::{extract_document_node, extract_exports_hash, extract_raw_dependencies};
pub use graph::{DependencyGraph, RemovedRecord, compute_affected_paths};
pub use model::{
    AffectedPaths, Backlink, Change, Dependency, DependencyKind, DocumentNode, fnv1a_hash,
};
