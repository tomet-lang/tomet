//! =[ tomet-validator ]
//!
//! Read-only `.tmt` document schema, blueprint, and lint validation facade.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Read-Only In-Memory Validation)
//!   | `tomet-validator` performs pure, read-only semantic and schema validation across
//!   | an in-memory [`Document`]. It performs zero I/O, no reference resolution (handled
//!   | by `tomet-address` and `tomet-links`), and no AST mutations.
//! - (Facade Architecture)
//!   | Re-exports and unifies:
//!   | - `tomet-validator-core`: Base semantic element, ID duplicate, and argument checks.
//!   | - `tomet-validator-rule`: Node constraint and `:rule` connect validation engine.
//!   | - `tomet-validator-blueprint`: Document structural blueprint extraction and conformance.

pub use tomet_validator_blueprint as blueprint;
pub use tomet_validator_blueprint::*;
pub use tomet_validator_core::*;
pub use tomet_validator_rule as rule;
pub use tomet_validator_rule::*;

use tomet_ast::Document;
use tomet_semantics::Bindings;

/// Runs all validation rules against a parsed `Document` and returns every
/// violation found. Read-only: never mutates `doc`, never does I/O.
pub fn validate_document(doc: &Document) -> Vec<Diagnostic> {
    validate_document_with(doc, &Bindings::default())
}

/// [`validate_document`], with the namespaces the document has in scope.
pub fn validate_document_with(doc: &Document, bindings: &Bindings) -> Vec<Diagnostic> {
    let mut errors = Vec::new();

    // 1. Core element, ID, region, argument, and anchor checks
    validate_core(doc, bindings, &mut errors);

    // 2. Local :rule connect constraint checks
    check_rule_connects(doc, bindings, &mut errors);

    errors
}

#[cfg(test)]
mod tests;
