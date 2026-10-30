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
    /// in scope, the blueprint its `::kind` names (if the vault declares one),
    /// and the `type:` the settings declare for its `::meta` fields, and no
    /// setting that only the vault config may make is written anywhere but there. Then a
    /// copy of it is prepared, to report what could not be resolved; `doc`
    /// itself is not changed, so the spans in the findings are the ones in the
    /// text the caller has.
    pub fn findings(&self, path: &Path, doc: &Document) -> Findings {
        // The blueprint for the document's kind, if the vault declares one:
        // the one part of the check that needs the disk.
        let blueprint = tomet_declaration::document_kind(doc)
            .filter(|kind| kind != "blueprint")
            .and_then(|kind| {
                tomet_workspace::blueprint::find_blueprint(&self.root, &kind, &self.config)
            })
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|src| tomet_parser::parse_document(&src).ok());
        let mut diagnostics = self.diagnostics(doc, blueprint.as_slice());

        // What only the vault config may set has no effect anywhere else. This
        // is the one check that needs the path: the validator cannot tell a
        // vault config from any other document by what is in it.
        if !may_set_vault_only(path, doc) {
            tomet_validator::check_vault_only_settings(
                doc,
                tomet_config::is_vault_only_key,
                &mut diagnostics,
            );
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
        let mut unresolved = self.prepare(&mut prepared, path).unresolved;
        // A blueprint is written to be filled in: what only has a value when a
        // document is being created from it (`generate.*`, a variable with no
        // default) is not a mistake there.
        if tomet_declaration::document_kind(doc).as_deref() == Some("blueprint") {
            unresolved.retain(|u| !u.creation_only);
        }

        Findings {
            diagnostics,
            unresolved,
        }
    }
}

/// Whether `doc`, read from `path`, may write what only the vault config sets.
///
/// The vault config may. So may a config document that is not a layer of any
/// stack -- a sample or a template under another name -- because nothing reads it,
/// so there is nothing for the setting to fail to affect. A folder config
/// ([`tomet_load_config::FOLDER_CONFIG_FILE`]) and an ordinary document are the
/// layers it would be wrongly written in.
fn may_set_vault_only(path: &Path, doc: &Document) -> bool {
    let name = path.file_name().and_then(|name| name.to_str());
    if name == Some(tomet_load_config::VAULT_CONFIG_FILE) {
        return true;
    }
    name != Some(tomet_load_config::FOLDER_CONFIG_FILE)
        && tomet_declaration::document_kind(doc).as_deref() == Some("config")
}
