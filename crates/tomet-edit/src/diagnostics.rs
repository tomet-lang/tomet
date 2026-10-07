//! Post-edit diagnostics over an [`EditDoc`].
//!
//! A collaborative editing runtime (e.g. CRDT) merges concurrent edits without
//! rejecting writes. After applying edits, this module exports the document back
//! to source text, parses it with the AST parser (`parse_document`), and runs
//! `validate_document` to report duplicate IDs or validation warnings.

use tomet_parser::{Error as ParseError, parse_document};
use tomet_validator::{Diagnostic, validate_document};

use crate::doc::EditDoc;
use crate::export::export_doc;

/// The diagnostic report of an edited document.
#[derive(Debug)]
pub enum PostEditReport {
    /// The document failed to parse into an AST.
    ParseFailure(ParseError),
    /// The document parsed cleanly and yielded semantic diagnostics.
    Validation(Vec<Diagnostic>),
}

/// Diagnoses an [`EditDoc`] by exporting to source, parsing with [`parse_document`],
/// and running [`validate_document`].
pub fn diagnose_doc(doc: &EditDoc) -> PostEditReport {
    let source = export_doc(doc);
    match parse_document(&source) {
        Ok(ast) => PostEditReport::Validation(validate_document(&ast)),
        Err(err) => PostEditReport::ParseFailure(err),
    }
}
