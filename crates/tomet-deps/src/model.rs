//! Pure in-memory data models for document dependencies, backlinks, and invalidation.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Standard 64-bit FNV-1a hash for content and metadata fingerprinting.
pub fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// The nature of a directed dependency between two documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DependencyKind {
    /// A normal hypertext or reference link (e.g. `[[target]]`, `@link("target")`, `ref:target`).
    Link,
    /// An embedded transclusion (e.g. `@embed("target")`, `![]`, `${...}`).
    /// Content alterations in the target directly invalidate the referencing document.
    Embed,
    /// A metadata-level dependency (referencing another document's exported title/metadata).
    Metadata,
}

/// A directed reference from one document to another.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Dependency {
    /// Target document identifier or path.
    pub target: String,
    /// The dependency relationship kind.
    pub kind: DependencyKind,
    /// The raw string target as written in source (e.g. `"Notes/Intro"`).
    pub raw: String,
}

/// An incoming backlink representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Backlink {
    /// The document containing the link.
    pub source: String,
    /// The target document receiving the link.
    pub target: String,
    /// The dependency relationship kind.
    pub kind: DependencyKind,
}

/// A document node containing its dependency signature and outgoing references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentNode {
    /// Path or canonical identifier of the document.
    pub path: String,
    /// Hash of the source text content.
    pub content_hash: u64,
    /// Hash of exported declarations / metadata (title, kind, etc.).
    pub exports_hash: u64,
    /// Outgoing dependencies to other documents.
    pub outgoing: Vec<Dependency>,
}

/// An observed change to a document within a set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Only document text modified; outgoing dependencies and exported metadata did not change.
    ContentOnly(String),
    /// Document was updated with potential dependency or export changes.
    Modified(DocumentNode),
    /// A document was newly added.
    Added(DocumentNode),
    /// A document was removed.
    Removed(String),
}

/// Impact and invalidation report produced by dependency analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AffectedPaths {
    /// Documents directly modified or indirectly affected by transclusions, backlink changes, or metadata updates.
    pub rebuild_required: HashSet<String>,
    /// Documents whose incoming backlink list changed.
    pub backlinks_changed: HashSet<String>,
    /// Documents whose exported metadata changed.
    pub exports_changed: HashSet<String>,
    /// Documents whose source content was directly modified.
    pub modified: HashSet<String>,
    /// Documents newly added.
    pub added: HashSet<String>,
    /// Documents removed.
    pub removed: HashSet<String>,
}

impl AffectedPaths {
    /// Returns true if no documents require rebuilding.
    pub fn is_empty(&self) -> bool {
        self.rebuild_required.is_empty()
            && self.modified.is_empty()
            && self.added.is_empty()
            && self.removed.is_empty()
    }
}
