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
//! =[ wasm ]
//!
//! This crate, and `tomet` and `tomet-load` above it, compile for
//! `wasm32-unknown-unknown` (the host supplies `getrandom`'s `js` feature,
//! as `bindings/js` does). Compiling is not running: on that target
//! `std::fs` returns errors, so `Vault::discover` is for hosts that have a
//! disk. Measured on `bindings/js` (release, `wasm-opt -Oz`), routing a
//! parse through `from_sources` added about 1% to the module and reaching
//! `tomet-load` as well added about 0.3% more.
//!
//! =[ Preparing ]
//!
//! [`Vault::prepare`] resolves every `${...}` in a document, once, against a
//! [`Resolver`] the caller supplies. What the vault adds is what only it
//! knows: the macros in scope, the settings' `type:` declarations that make
//! `.meta` typed, and the document being prepared standing for its own path
//! as it was written. Where the other documents come from -- the disk, a
//! host's memory -- is the caller's resolver; `tomet-load` supplies the disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tomet_ast::{Block, Document, Span};
use tomet_config::PrinterConfig;
use tomet_expr::{
    DocRef, EvalContext, EvalError, EvalValue, Phase, PreparedBlocks, SystemGenerate,
};
use tomet_semantics::{Bindings, LoadedVocabularies};

mod resolve;

pub use resolve::{Overlay, typed_meta};
pub use tomet_expr::{DirRef, MemoryResolver, NoResolver, Resolver};
pub use tomet_transform::{Unresolved, UnresolvedKind};
pub use tomet_validator::Diagnostic;

/// What [`Vault::prepare`] could not do.
///
/// `unresolved` is not an error: an expression with no value is left in
/// the output as written, so a document showing `${...}` as an example
/// still renders. `tomet check` reports the list as warnings, which is
/// what tells a typo apart from an example.
#[derive(Debug, Default)]
pub struct Prepared {
    pub unresolved: Vec<Unresolved>,
}

/// Validates `source` the way a host with no disk can: what `tomet check` says
/// of it, given the vault's pieces as text.
///
/// - `vocabularies`: `::vocabulary(ns)` documents. One that does not parse, has
///   no header, or claims a reserved or already-taken namespace is skipped: it
///   binds no namespace.
/// - `config`: the settings, as a config document (what `default.config.tmt`
///   holds). Without it the defaults are used, and nothing declares a `type:`.
///   One that does not parse is an error: a host that passes settings means
///   them.
/// - `blueprints`: `::blueprint(kind)` documents. A document is checked against
///   the one for its `::kind`, if there is one. One that does not parse is
///   skipped.
///
/// An expression that reaches another document is not reported, since no
/// document is given to reach: it would come back unresolved every time.
pub fn validate_sources(
    source: &str,
    vocabularies: &[String],
    config: Option<&str>,
    blueprints: &[String],
) -> Result<Vec<Diagnostic>, String> {
    let config = match config {
        Some(text) => {
            tomet_config::load_config_from_str(text).map_err(|e| format!("config: {e}"))?
        }
        None => PrinterConfig::default(),
    };
    let labels: Vec<String> = (0..vocabularies.len())
        .map(|i| format!("vocabularies[{i}]"))
        .collect();
    let sources: Vec<(&str, &str)> = labels
        .iter()
        .map(String::as_str)
        .zip(vocabularies.iter().map(String::as_str))
        .collect();
    let vault = Vault::from_sources(config, ".", &sources);
    let (doc, _) = vault.parse(source).map_err(|e| e.to_string())?;
    let blueprints: Vec<Document> = blueprints
        .iter()
        .filter_map(|src| tomet_parser::parse_document(src).ok())
        .collect();
    Ok(vault.diagnostics(&doc, &blueprints))
}

/// What an evaluation in one document is done in. See [`Vault::scope`].
struct Scope<'a> {
    overlay: Overlay<'a>,
    macros: std::collections::HashMap<String, String>,
    chain: Vec<DocRef>,
    self_doc: DocRef,
    vars: EvalValue,
}

impl Scope<'_> {
    fn context(&self) -> EvalContext<'_> {
        EvalContext {
            phase: Phase::Output,
            resolver: &self.overlay,
            generate: &SystemGenerate,
            self_doc: self.self_doc.clone(),
            macros: &self.macros,
            vars: &self.vars,
            includes: &self.chain,
            heading_level: 0,
            notes: Default::default(),
        }
    }
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
        let mut loaded = LoadedVocabularies::default();
        for (label, src) in vocabularies {
            match tomet_parser::parse_document(src) {
                Ok(doc) => loaded.add(label, &doc),
                Err(e) => loaded
                    .errors
                    .push(format!("declared vocabulary {label}: {e}")),
            }
        }
        Self::new(config, root.into(), loaded)
    }

    /// What checking finds wrong with `doc`, without reading anything: the
    /// validator's diagnostics against the vocabularies it has in scope, the
    /// blueprint its `::kind` names if one of `blueprints` is for that kind, and
    /// the `type:` the settings declare for its `::meta` fields.
    ///
    /// This is the part of `tomet check` that needs no disk, so a host with
    /// none -- the bindings in a browser -- gets the same answer as the CLI
    /// gives. What does need it (an expression that reaches another document,
    /// finding the blueprint's file) is added by whoever has one:
    /// `tomet-load`'s `Vault::findings`.
    pub fn diagnostics(&self, doc: &Document, blueprints: &[Document]) -> Vec<Diagnostic> {
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
            && let Some(blueprint) = blueprints
                .iter()
                .find(|b| tomet_semantics::document_blueprint(b).as_deref() == Some(kind.as_str()))
        {
            diagnostics.extend(tomet_validator::validate_against_blueprint_with(
                doc, blueprint, &bindings,
            ));
        }
        diagnostics
    }

    /// The names in scope for `doc`: `std`, the document's own `::kind`,
    /// and anything it brings in with `::use`.
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

    /// Resolves every `${...}` in `doc`, so it is the shape a converter
    /// should see.
    ///
    /// `path` is where the document sits, which is what `self` is.
    /// References -- `#id`, `self.meta` -- are answered from the document as
    /// it was written, not as this call rewrites it; everything else is
    /// asked of `resolver`.
    ///
    /// Call this and not the underlying pass directly. A caller reaching
    /// past it for one rewrite is how a rewrite ends up applied in three
    /// commands and missing from the fourth -- which is the state
    /// interpolation was in before this existed.
    pub fn prepare(&self, doc: &mut Document, path: &Path, resolver: &dyn Resolver) -> Prepared {
        let scope = self.scope(doc, path, resolver);
        Prepared {
            unresolved: tomet_transform::resolve_expressions(doc, &scope.context()),
        }
    }

    /// What `expr`, written in `doc` (which sits at `path`), comes to when the
    /// document is rendered: in the same scope [`Vault::prepare`] evaluates
    /// every expression of it in, so what an editor shows for one expression
    /// is what the output will show.
    ///
    /// The document is read as written (`#id`, `self.meta`), everything else
    /// is asked of `resolver`. `generate.*` and `vars` fail, as they do when
    /// rendering: they exist only when a document is created.
    pub fn evaluate(
        &self,
        doc: &Document,
        path: &Path,
        expr: &tomet_ast::InterpExpr,
        resolver: &dyn Resolver,
    ) -> Result<EvalValue, EvalError> {
        let scope = self.scope(doc, path, resolver);
        tomet_expr::evaluate(expr, &scope.context())
    }

    /// Everything an evaluation of something written in `doc` needs that is
    /// not the expression: where `self` is, what `#id` is answered from, the
    /// macros in scope.
    fn scope<'a>(&self, doc: &Document, path: &Path, resolver: &'a dyn Resolver) -> Scope<'a> {
        let self_doc = self.doc_ref(path);
        let snapshot = Arc::new(doc.clone());
        Scope {
            overlay: Overlay {
                meta: typed_meta(&self.config, &snapshot, &self_doc),
                path: self_doc.clone(),
                snapshot,
                inner: resolver,
            },
            macros: self.evaluation_config(doc).macros,
            // The document is the first of the chain of files being included,
            // so a file that includes it back is a cycle at once, not one lap
            // later.
            chain: vec![self_doc.clone()],
            self_doc,
            vars: EvalValue::Null,
        }
    }

    /// Prepares `blocks`, which were read from the document `doc`, the way that
    /// document is prepared on its own: its expressions in its own context, its
    /// macros, its `self`, its settings. This is what `include` asks of the
    /// resolver, so that an included file means what it means in its own place.
    ///
    /// What could not be resolved inside is left in the blocks as written. It is
    /// reported in full only when that file is checked on its own, so here it
    /// is counted: the including file says that it brought some in.
    pub fn prepare_blocks(
        &self,
        doc: &DocRef,
        blocks: Vec<Block>,
        includes: &[DocRef],
        resolver: &dyn Resolver,
    ) -> Result<PreparedBlocks, String> {
        let source = resolver.document(doc)?;
        let overlay = Overlay {
            meta: typed_meta(&self.config, &source, doc),
            path: doc.clone(),
            snapshot: source.clone(),
            inner: resolver,
        };
        let macros = self.evaluation_config(&source).macros;
        let cx = EvalContext {
            phase: Phase::Output,
            resolver: &overlay,
            generate: &SystemGenerate,
            self_doc: doc.clone(),
            macros: &macros,
            vars: &EvalValue::Null,
            includes,
            heading_level: 0,
            notes: Default::default(),
        };
        let mut holder = Document::new(blocks, Span::default());
        let unresolved = tomet_transform::resolve_expressions(&mut holder, &cx).len();
        Ok(PreparedBlocks {
            blocks: holder.blocks,
            unresolved,
        })
    }

    /// `path` as the vault names a document: measured from the root, with
    /// `/` separators and no leading `./`.
    pub fn doc_ref(&self, path: &Path) -> DocRef {
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        DocRef(rel.strip_prefix("./").unwrap_or(&rel).to_string())
    }

    /// The macros in scope for `doc`: its own `::config`, then the vault's
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
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUEPRINT: &str = "::kind(blueprint)\n::blueprint(note)\n::meta{\n  date: \"2026\"\n}\n\n=[ Items ]#(items):rule(allow: list(task))\n";
    const UUID_SETTINGS: &str = "::kind(config)\n::config{ meta: { id: { type: uuid } } }\n";

    fn texts(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// The settings a host passes are in force: without them nothing declares
    /// a `type:`, with them a field that has the wrong form is reported.
    #[test]
    fn settings_given_as_text_declare_the_types_of_fields() {
        let source = "::meta{ id: not-a-uuid }\n";
        assert!(validate_sources(source, &[], None, &[]).unwrap().is_empty());

        let found = validate_sources(source, &[], Some(UUID_SETTINGS), &[]).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].to_string().contains("not a valid uuid"));
    }

    /// A document is checked against the blueprint for its `::kind`, if the
    /// host passed one, and against none otherwise.
    #[test]
    fn a_blueprint_given_as_text_is_checked_against() {
        let bad = "::kind(note)\n::meta{ date: \"2026-10-13\" }\n\n=[ Items ]#(items)\n@heading[ Disallowed ]\n";
        let good = "::kind(note)\n::meta{ date: \"2026-10-13\" }\n\n=[ Items ]#(items)\n@task[ Clean room ]\n";
        let blueprints = texts(&[BLUEPRINT]);

        assert!(
            !validate_sources(bad, &[], None, &blueprints)
                .unwrap()
                .is_empty()
        );
        assert!(
            validate_sources(good, &[], None, &blueprints)
                .unwrap()
                .is_empty()
        );
        // Nothing passed, nothing to hold it to.
        assert!(validate_sources(bad, &[], None, &[]).unwrap().is_empty());
        // A blueprint for another kind does not apply.
        let other = texts(&[
            "::kind(blueprint)\n::blueprint(memo)\n\n=[ Items ]#(items):rule(allow: list(task))\n",
        ]);
        assert!(validate_sources(bad, &[], None, &other).unwrap().is_empty());
    }

    /// Settings that do not read are an error, not the defaults: a host that
    /// passes them means them. A blueprint that does not read is skipped, like
    /// a vocabulary.
    #[test]
    fn settings_that_do_not_read_are_an_error_and_a_bad_blueprint_is_skipped() {
        let err =
            validate_sources("text\n", &[], Some("@caution[ unterminated\n"), &[]).unwrap_err();
        assert!(err.starts_with("config:"), "{err}");
        assert!(
            validate_sources("text\n", &[], None, &texts(&["@caution[ unterminated\n"])).is_ok()
        );
    }

    /// The vocabularies still bind, and an expression that reaches another
    /// document is not reported -- there is none to reach.
    #[test]
    fn vocabularies_still_bind_and_a_reaching_expression_is_not_reported() {
        let vocab = texts(&[ACME]);
        assert!(
            validate_sources("::kind(acme)\n", &vocab, None, &[])
                .unwrap()
                .is_empty()
        );
        assert!(
            !validate_sources("@nonesuch{}\n", &[], None, &[])
                .unwrap()
                .is_empty()
        );
        assert!(
            validate_sources("${tmt(\"other.tmt\").meta.title}\n", &[], None, &[])
                .unwrap()
                .is_empty()
        );
    }

    const ACME: &str = "::kind(vocabulary)\n::vocabulary(acme){ version: \"1.0.0\" }\n";

    #[test]
    fn a_vocabulary_handed_in_as_text_binds_like_one_read_from_disk() {
        let vault = Vault::from_sources(PrinterConfig::default(), ".", &[("acme.tmt", ACME)]);
        assert!(vault.vocabulary_errors.is_empty());

        let (_, bindings) = vault.parse("::kind(acme)\n").expect("parses");
        assert!(bindings.kind.is_some());
    }

    #[test]
    fn a_bad_vocabulary_is_reported_under_its_label_and_not_fatal() {
        let vault = Vault::from_sources(
            PrinterConfig::default(),
            ".",
            &[(
                "doc.tmt",
                "::kind(vocabulary)\n::vocabulary(doc){ version: \"1.0.0\" }\n",
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
        let (_, bindings) = vault.parse("::kind(acme)\n").expect("parses");
        assert!(bindings.kind.is_none());
    }

    fn prepared(vault: &Vault, path: &str, src: &str) -> (Document, Prepared) {
        let (mut doc, _) = vault.parse(src).expect("parses");
        let prepared = vault.prepare(&mut doc, Path::new(path), &NoResolver);
        (doc, prepared)
    }

    /// The text of every paragraph, joined by a space. Two expressions on
    /// one line are two blocks, so a document is read as a whole.
    fn text_of(doc: &Document) -> String {
        doc.blocks
            .iter()
            .filter_map(|block| match block {
                tomet_ast::Block::Paragraph(p) => Some(
                    p.content
                        .iter()
                        .filter_map(|i| match i {
                            tomet_ast::Inline::Text(t) => Some(t.value.as_str()),
                            _ => None,
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn prepare_measures_self_path_from_the_root() {
        let vault = Vault::from_sources(PrinterConfig::default(), "/v", &[]);
        let (doc, prepared) = prepared(
            &vault,
            "/v/notes/a.tmt",
            "at ${self.path} (${self.filename})\n",
        );
        assert!(prepared.unresolved.is_empty(), "{:?}", prepared.unresolved);
        assert_eq!(text_of(&doc), "at notes/a.tmt (a.tmt)");
    }

    #[test]
    fn a_vault_in_the_current_directory_names_a_document_without_a_leading_dot() {
        let vault = Vault::from_sources(PrinterConfig::default(), ".", &[]);
        assert_eq!(vault.doc_ref(Path::new("./a.tmt")), DocRef("a.tmt".into()));
        assert_eq!(vault.doc_ref(Path::new("a.tmt")), DocRef("a.tmt".into()));
    }

    #[test]
    fn a_document_reads_its_own_meta_typed_by_the_settings() {
        let config =
            tomet_config::load_config_from_str("::settings{ meta: { tags: { type: list } } }\n")
                .unwrap();
        let vault = Vault::from_sources(config, "/v", &[]);
        // No `tags` written: still a list, so `.contains` answers instead of failing.
        let (doc, prepared) = prepared(
            &vault,
            "/v/a.tmt",
            "::meta{ title: A }\n\nrust? ${self.meta.tags.contains(\"rust\")}\n",
        );
        assert!(prepared.unresolved.is_empty(), "{:?}", prepared.unresolved);
        assert_eq!(text_of(&doc), "rust? false");
    }

    #[test]
    fn macros_from_the_vault_and_the_document_are_both_in_scope() {
        let config = tomet_config::load_config_from_str(
            "::settings{ macros: { gh: \"https://gh/${1}\", site: \"vault\" } }\n",
        )
        .unwrap();
        let vault = Vault::from_sources(config, "/v", &[]);
        let (doc, prepared) = prepared(
            &vault,
            "/v/a.tmt",
            "::config{ macros: { site: \"doc\" } }\n\n${macro.gh(1)} ${macro.site()}\n",
        );
        assert!(prepared.unresolved.is_empty(), "{:?}", prepared.unresolved);
        // The document wins a collision.
        assert_eq!(text_of(&doc), "https://gh/1 doc");
    }

    #[test]
    fn references_are_answered_from_the_document_as_written() {
        let vault = Vault::from_sources(PrinterConfig::default(), "/v", &[]);
        // `n` is itself an expression. The reference sees it unresolved, so it
        // is not the number 3 -- evaluation order cannot change an answer.
        let (doc, prepared) = prepared(
            &vault,
            "/v/a.tmt",
            "@card(n: ${add(1, 2)})#(c1)\n\nn is ${#c1.prop.n}\n",
        );
        assert_eq!(prepared.unresolved.len(), 1, "{:?}", prepared.unresolved);
        assert_eq!(text_of(&doc), "n is ${#c1.prop.n}");
    }
}
