//! What checking finds in one document: the one place `tomet check`,
//! `tomet stats` and the language server get it from.
//!
//! It used to be assembled inside `apps/cli`, which left every other caller
//! to put the same four steps together again -- or not to, and silently
//! report less. A check added here reaches all of them.

use std::path::Path;

use tomet_ast::Document;

use crate::{Unresolved, Vault};

/// What checking finds in one document that parsed: the validator's
/// diagnostics, and the `${...}` that had no value.
///
/// It is what `check` reports and what `stats` counts, so the two agree
/// about which documents fail and how many warnings there are.
pub struct Findings {
    pub diagnostics: Vec<tomet_validator::Diagnostic>,
    pub unresolved: Vec<Unresolved>,
}

impl Findings {
    /// Whether checking fails this document.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(is_error)
    }

    /// How many warnings checking reports for this document.
    pub fn warnings(&self) -> usize {
        self.unresolved.len() + self.diagnostics.iter().filter(|d| !is_error(d)).count()
    }
}

/// A warning is the document telling you about itself -- `@draft`,
/// `@fixme`. It is reported and does not fail the run: marking a gap has
/// to stay cheaper than leaving it unmarked.
pub fn is_error(d: &tomet_validator::Diagnostic) -> bool {
    d.severity() == tomet_validator::Severity::Error
}

impl Vault {
    /// What checking finds in `doc`, which was read from `path` in this vault.
    ///
    /// The document is validated as written, against the vocabularies it has
    /// in scope, the blueprint its `@kind` names (if the vault declares one),
    /// and the `type:` the settings declare for its `@meta` fields. Then a
    /// copy of it is prepared, to report what could not be resolved; `doc`
    /// itself is not changed, so the spans in the findings are the ones in the
    /// text the caller has.
    pub fn findings(&self, path: &Path, doc: &Document) -> Findings {
        let bindings = self.bindings(doc);
        let mut diagnostics = tomet_validator::validate_document_with(doc, &bindings);

        // The settings' `type:` declarations, which the validator is handed
        // as plain data -- it sits below the crate that reads settings.
        let field_types = self
            .config
            .meta
            .fields
            .iter()
            .filter_map(|(name, field)| Some((name.clone(), field.field_type.clone()?)))
            .collect();
        tomet_validator::check_field_types(doc, &field_types, &mut diagnostics);

        if let Some(kind) = tomet_semantics::document_kind(doc)
            && kind != "blueprint"
            && let Some(blueprint_path) =
                tomet_workspace::blueprint::find_blueprint(&self.root, &kind, &self.config)
            && let Ok(src) = std::fs::read_to_string(&blueprint_path)
            && let Ok(blueprint_doc) = tomet_parser::parse_document(&src)
        {
            diagnostics.extend(tomet_validator::validate_against_blueprint_with(
                doc,
                &blueprint_doc,
                &bindings,
            ));
        }

        // A `${...}` with no value is not an error: a document showing one
        // as an example is a legitimate document, and `export` leaves it
        // in the output as written. Saying so here is what tells that
        // apart from a typo, which otherwise renders as itself and is
        // never mentioned by anything.
        //
        // Run on a copy: this reports on the document as written, and
        // `prepare` rewrites the tree.
        let mut prepared = doc.clone();
        let unresolved = self.prepare(&mut prepared, path).unresolved;

        Findings {
            diagnostics,
            unresolved,
        }
    }
}
