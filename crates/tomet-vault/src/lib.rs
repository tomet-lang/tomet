//! A vault's config and vocabularies, and what can be done with them
//! without touching the disk.
//!
//! A [`Vault`] is the state a document is read against: the config that
//! governs it and the vocabularies that config declares. Given that state,
//! parsing a source and binding its names ([`Vault::parse`],
//! [`Vault::bindings`]) and rewriting a document into the shape a converter
//! should see ([`Vault::prepare`]) are plain computation.
//!
//! What this crate does not do is find that state. Where the config sits
//! and which files hold the vocabularies is a question about a filesystem,
//! and `tomet-load` answers it and then builds a `Vault` from the result.
//! A caller that already has the pieces in memory -- a wasm host, or a
//! binding handed vocabulary text -- builds one directly with
//! [`Vault::from_sources`] and gets the same verdicts, because both go
//! through the same rules (`LoadedVocabularies::add` in `tomet-semantics`).
//!
//! # wasm
//!
//! This crate, and `tomet` and `tomet-load` above it, compile for
//! `wasm32-unknown-unknown` (the host supplies `getrandom`'s `js` feature,
//! as `bindings/js` does). Compiling is not running: on that target
//! `std::fs` returns errors, so `Vault::discover` is for hosts that have a
//! disk. Measured on `bindings/js` (release, `wasm-opt -Oz`), routing a
//! parse through `from_sources` added about 1% to the module and reaching
//! `tomet-load` as well added about 0.3% more.
//!
//! # Preparing
//!
//! `${filter(...)}` in an `@kind(doc.index)` document is answered from a
//! table of every document in the vault. Building that table is a walk of
//! the disk, so [`Vault::prepare`] takes it as a closure that is only
//! called when a document actually asks: a document with no query pays
//! nothing, and a caller with no table passes an empty one and gets the
//! query left unexpanded.

use std::path::{Path, PathBuf};

use tomet_ast::Document;
use tomet_config::PrinterConfig;
use tomet_resolver::{LoadedVocabularies, load_vocabulary_sources};
use tomet_semantics::Bindings;

pub use tomet_search::{IndexQueryError, IndexRow, is_index_document};
pub use tomet_transform::Unresolved;

/// What [`Vault::prepare`] did, and what it could not do.
///
/// `unresolved` is not an error: an expression with no value is left in
/// the output as written, so a document showing `${...}` as an example
/// still renders. `tomet check` reports the list as warnings, which is
/// what tells a typo apart from an example.
#[derive(Debug, Default)]
pub struct Prepared {
    pub expanded_queries: usize,
    pub unresolved: Vec<Unresolved>,
}

/// A vault's config and the vocabularies it declares, resolved once.
///
/// "Vault" is the directory the config file sits in. Whether `root` names
/// a directory that exists is not this type's concern; it only decides
/// what `${self.path}` is measured from.
pub struct Vault {
    /// The config governing this vault, or the defaults when there is none.
    pub config: PrinterConfig,
    /// The directory declared vocabulary paths are relative to, and the
    /// one `${self.path}` is measured from.
    pub root: PathBuf,
    /// Vocabularies that were declared but could not be accepted. Not
    /// fatal: a caller reports them and carries on, because one bad
    /// vocabulary should not hide the state of every document.
    pub vocabulary_errors: Vec<String>,
    loaded: LoadedVocabularies,
}

impl Vault {
    /// A vault over vocabularies that have already been loaded.
    pub fn new(config: PrinterConfig, root: PathBuf, loaded: LoadedVocabularies) -> Self {
        Self {
            config,
            root,
            vocabulary_errors: loaded.errors.clone(),
            loaded,
        }
    }

    /// A vault whose vocabularies are given as `(label, source text)`
    /// pairs. Reads nothing.
    pub fn from_sources(
        config: PrinterConfig,
        root: impl Into<PathBuf>,
        vocabularies: &[(&str, &str)],
    ) -> Self {
        Self::new(config, root.into(), load_vocabulary_sources(vocabularies))
    }

    /// The names in scope for `doc`: `std`, the document's own `@kind`,
    /// and anything it brings in with `@use`.
    ///
    /// This is what makes an element name mean something. Matching on a
    /// bare `Sigil::Named` instead is the shortcut that drops the
    /// namespaced spelling of the same element.
    pub fn bindings(&self, doc: &Document) -> Bindings {
        self.loaded.bindings_for(doc)
    }

    /// Parses `src` and binds its names against this vault.
    pub fn parse(&self, src: &str) -> Result<(Document, Bindings), tomet_parser::Error> {
        let doc = tomet_parser::parse_document(src)?;
        let bindings = self.bindings(&doc);
        Ok((doc, bindings))
    }

    /// Rewrites `doc` into the shape a converter should see.
    ///
    /// Two rewrites, in this order, because the first produces elements
    /// and the second turns expressions into text:
    ///
    /// 1. index queries -- `${filter(...)}` becomes the `@link(ref:...)`
    ///    entries it selects, answered from `rows`. `rows` is called only
    ///    for an `@kind(doc.index)` document.
    /// 2. interpolation -- every remaining `${...}` becomes the text it
    ///    stands for, against this vault's macros and this document's
    ///    position.
    ///
    /// `path` is where the document sits, which is what `${self.path}`
    /// and `${self.filename}` are about.
    ///
    /// Call this and not the underlying passes directly. A caller reaching
    /// past it for one rewrite is how a rewrite ends up applied in three
    /// commands and missing from the fourth -- which is the state
    /// interpolation was in before this existed.
    pub fn prepare<'r>(
        &self,
        doc: &mut Document,
        path: &Path,
        rows: impl FnOnce() -> &'r [IndexRow],
    ) -> Result<Prepared, IndexQueryError> {
        let mut expanded_queries = 0;
        if is_index_document(doc) {
            expanded_queries = tomet_transform::expand_index_queries(doc, rows())?;
        }

        let config = self.evaluation_config(doc);
        let vars = self.evaluation_vars(path);
        let unresolved = tomet_transform::resolve_interpolations(doc, &config, &vars);

        Ok(Prepared {
            expanded_queries,
            unresolved,
        })
    }

    /// The macros in scope for `doc`: its own `@config`, then the vault's
    /// for every name it did not claim.
    ///
    /// The document wins a collision, which is the direction every other
    /// scope in this format runs. Before this lived here it lived in the
    /// CLI, so a macro reached CommonMark and nothing else.
    fn evaluation_config(&self, doc: &Document) -> tomet_semantics::DocumentConfig {
        let mut config = tomet_semantics::document_config(doc);
        for (name, template) in &self.config.macros {
            config
                .macros
                .entry(name.clone())
                .or_insert_with(|| template.clone());
        }
        config
    }

    /// `${self.path}` and `${self.filename}`.
    ///
    /// `path` is measured from this vault's root, `filename` is the
    /// basename. Two fields and not one: naming either for the other's
    /// meaning is how a field starts lying.
    fn evaluation_vars(&self, path: &Path) -> tomet_compute::EvaluationContext {
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let rel = rel.strip_prefix("./").unwrap_or(&rel).to_string();
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        tomet_compute::EvaluationContext::new().with_var(
            "self",
            tomet_ast::Value::Map(vec![
                ("path".to_string(), tomet_ast::Value::String(rel)),
                ("filename".to_string(), tomet_ast::Value::String(filename)),
            ]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACME: &str = "@kind(vocabulary)\n@vocabulary(acme){ version: \"1.0.0\" }\n";

    #[test]
    fn a_vocabulary_handed_in_as_text_binds_like_one_read_from_disk() {
        let vault = Vault::from_sources(PrinterConfig::default(), ".", &[("acme.tmt", ACME)]);
        assert!(vault.vocabulary_errors.is_empty());

        let (_, bindings) = vault.parse("@kind(acme)\n").expect("parses");
        assert!(bindings.kind.is_some());
    }

    #[test]
    fn a_bad_vocabulary_is_reported_under_its_label_and_not_fatal() {
        let vault = Vault::from_sources(
            PrinterConfig::default(),
            ".",
            &[(
                "doc.tmt",
                "@kind(vocabulary)\n@vocabulary(doc){ version: \"1.0.0\" }\n",
            )],
        );
        assert!(
            vault
                .vocabulary_errors
                .iter()
                .any(|e| e.contains("doc.tmt")),
            "{:?}",
            vault.vocabulary_errors
        );
        assert!(vault.parse("plain\n").is_ok());
    }

    #[test]
    fn without_a_vocabulary_the_kind_does_not_bind() {
        let vault = Vault::from_sources(PrinterConfig::default(), ".", &[]);
        let (_, bindings) = vault.parse("@kind(acme)\n").expect("parses");
        assert!(bindings.kind.is_none());
    }

    #[test]
    fn prepare_measures_self_path_from_the_root() {
        let vault = Vault::from_sources(PrinterConfig::default(), "/v", &[]);
        let (mut doc, _) = vault.parse("at ${self.path}\n").expect("parses");
        let prepared = vault
            .prepare(&mut doc, Path::new("/v/notes/a.tmt"), || &[])
            .expect("prepares");
        assert!(prepared.unresolved.is_empty(), "{:?}", prepared.unresolved);
        assert!(format!("{doc:?}").contains("notes/a.tmt"));
    }

    #[test]
    fn the_table_is_not_asked_for_by_a_document_with_no_query() {
        let vault = Vault::from_sources(PrinterConfig::default(), "/v", &[]);
        let (mut doc, _) = vault.parse("plain\n").expect("parses");
        vault
            .prepare(&mut doc, Path::new("/v/a.tmt"), || {
                panic!("the table was built for a document that asked nothing")
            })
            .expect("prepares");
    }
}
