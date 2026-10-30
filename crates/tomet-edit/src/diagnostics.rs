//! Post-edit diagnostics over an [`EditDoc`].
//!
//! A collaborative editing runtime (e.g. CRDT) merges concurrent edits without
//! rejecting writes. After applying edits, this module exports the document back
//! to source text, parses it with the AST parser (`parse_document`), and runs
//! `validate_document` to report duplicate IDs or validation warnings.

use tomet_ast::Document;
use tomet_parser::{Error as ParseError, parse_document};
use tomet_validator::{Diagnostic, validate_document};
use tomet_vault::Vault;

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
///
/// That knows `std` alone, as a document in a vault with no settings and no
/// vocabularies would; [`diagnose_doc_in`] is the report for a document in a vault.
pub fn diagnose_doc(doc: &EditDoc) -> PostEditReport {
    let source = export_doc(doc);
    match parse_document(&source) {
        Ok(ast) => PostEditReport::Validation(validate_document(&ast)),
        Err(err) => PostEditReport::ParseFailure(err),
    }
}

/// Diagnoses an [`EditDoc`] as a document of `vault`: what `tomet check`
/// finds without the disk ([`Vault::diagnostics`]) -- the vocabularies the
/// document has in scope, the types the vault's settings declare for `::meta`
/// fields, and the blueprint of `blueprints` that its `::kind` names.
pub fn diagnose_doc_in(doc: &EditDoc, vault: &Vault, blueprints: &[Document]) -> PostEditReport {
    let source = export_doc(doc);
    match parse_document(&source) {
        Ok(ast) => PostEditReport::Validation(vault.diagnostics(&ast, blueprints)),
        Err(err) => PostEditReport::ParseFailure(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::import_source;

    const SETTINGS: &str = "::kind(config)\n::config{ meta: { id: { type: uuid } } }\n";

    #[test]
    fn a_document_in_a_vault_is_held_to_the_types_its_settings_declare() {
        let doc = import_source("::id(not-a-uuid)\n");
        let plain = Vault::from_sources(Default::default(), ".", &[]);
        let typed = Vault::from_sources(
            tomet_config::load_config_from_str(SETTINGS).unwrap(),
            ".",
            &[],
        );

        let PostEditReport::Validation(without) = diagnose_doc_in(&doc, &plain, &[]) else {
            panic!("the document parses");
        };
        assert!(without.is_empty(), "{without:?}");

        let PostEditReport::Validation(with) = diagnose_doc_in(&doc, &typed, &[]) else {
            panic!("the document parses");
        };
        assert_eq!(with.len(), 1, "{with:?}");
        assert!(with[0].to_string().contains("not a valid uuid"));
    }
}
