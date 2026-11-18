//! A vault's config and vocabularies, and what can be done with them
//! without touching the disk.
//!
//! A [`Vault`] is the state a document is read against: the config that
//! governs it, the vocabularies that config declares, and the dictionaries
//! of terms it registers (`@term` is checked against them in
//! [`Vault::diagnostics`] and given the term's text in [`Vault::prepare`]). Given that state,
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
//! =[ A document that reads alone ]
//!
//! A document that declares `::profile(standalone)` is read as if the vault held nothing: the
//! macros and variables, the `type:` of a metadata field, the vocabularies, the dictionaries and
//! the other files an expression could reach are all taken away, so that what it means is what its
//! own bytes say. [`Vault::bindings`], [`Vault::prepare`], [`Vault::evaluate`] and the `@term`
//! check of [`Vault::diagnostics`] read this way. What a vault only checks and does not change the
//! meaning of (a blueprint, the form a metadata field must have) still applies.
//! [`Vault::prepare_selected`] does not: it is what makes a document keep that promise.
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
//! [`Vault::prepare`] resolves every `{$ ... }` in a document, once, against a
//! [`Resolver`] the caller supplies. What the vault adds is what only it
//! knows: the macros in scope, the settings' `type:` declarations that make
//! `.meta` typed, and the document being prepared standing for its own path
//! as it was written. Where the other documents come from -- the disk, a
//! host's memory -- is the caller's resolver; `tomet-load` supplies the disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tomet_ast::File;
use tomet_config::PrinterConfig;
use tomet_expr::{DocRef, EvalContext, EvalValue};
use tomet_semantics::{Bindings, LoadedDictionaries, LoadedVocabularies};
use tomet_span::Span;

mod resolve;

pub use resolve::{Overlay, typed_meta};
pub use tomet_expr::{DirRef, MemoryResolver, NoResolver, Resolver};
pub use tomet_validator::Diagnostic;

/// Whether `doc` promises `::profile(standalone)`, which is a promise to be read without a vault.
fn reads_alone<B>(doc: &File<B>) -> bool {
    tomet_declaration::document_profiles(doc).contains(&tomet_declaration::Profile::Standalone)
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
    let blueprints: Vec<File> = blueprints
        .iter()
        .filter_map(|src| tomet_ast_parser::parse_file(src).ok())
        .collect();
    Ok(vault.diagnostics(&doc, &blueprints))
}

/// What an evaluation in one document is done in. See [`Vault::scope`].
pub struct Scope<'a> {
    overlay: Overlay<'a>,
    macros: std::collections::HashMap<String, tomet_ast::Value>,
    chain: Vec<DocRef>,
    self_doc: DocRef,
    var: EvalValue,
    var_unset: Vec<String>,
}

impl Scope<'_> {
    pub fn context(&self) -> EvalContext<'_> {
        EvalContext {
            resolver: &self.overlay,
            self_doc: self.self_doc.clone(),
            macros: &self.macros,
            var: &self.var,
            var_unset: &self.var_unset,
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
/// what `$(self::path)` is measured from.
pub struct Vault {
    /// The config governing this vault, or the defaults when there is none.
    pub config: PrinterConfig,
    /// The directory declared vocabulary paths are relative to, and the
    /// one `$(self::path)` is measured from.
    pub root: PathBuf,
    /// Vocabularies that were declared but could not be accepted. Not
    /// fatal: a caller reports them and carries on, because one bad
    /// vocabulary should not hide the state of every document.
    pub vocabulary_errors: Vec<String>,
    /// Dictionaries that were declared but could not be accepted, and the terms of one that were
    /// left out. Not fatal either.
    pub dictionary_errors: Vec<String>,
    loaded: LoadedVocabularies,
    dictionaries: LoadedDictionaries,
    /// This vault as a document that promises `::profile(standalone)` sees it: nothing in it.
    /// Built when first asked for. See [`Vault::seen_by`].
    alone: std::sync::OnceLock<Box<Vault>>,
}

impl Vault {
    /// A vault over vocabularies that have already been loaded.
    pub fn new(config: PrinterConfig, root: PathBuf, loaded: LoadedVocabularies) -> Self {
        Self {
            config,
            root,
            vocabulary_errors: loaded.errors.clone(),
            dictionary_errors: Vec::new(),
            loaded,
            dictionaries: LoadedDictionaries::default(),
            alone: std::sync::OnceLock::new(),
        }
    }

    /// The vault and the resolver `doc` is read with: these, or, for a document that declares
    /// `::profile(standalone)`, the empty vault of [`Vault::seen_by`] and a resolver that finds no
    /// other file, so nothing but the document's own bytes answers an expression.
    fn reading<'a>(
        &'a self,
        doc: &File,
        resolver: &'a dyn Resolver,
    ) -> (&'a Vault, &'a dyn Resolver) {
        if reads_alone(doc) {
            (self.seen_by(doc), &NoResolver)
        } else {
            (self, resolver)
        }
    }

    /// The dictionaries `doc` looks its `@term` mentions up in: the vault's, or, for a document
    /// that reads alone, only itself, when it is a dictionary.
    fn dictionaries_for(&self, doc: &File) -> std::borrow::Cow<'_, LoadedDictionaries> {
        use std::borrow::Cow;
        if !reads_alone(doc) {
            return Cow::Borrowed(&self.dictionaries);
        }
        let mut own = LoadedDictionaries::default();
        if tomet_declaration::document_dictionary(doc).is_some() {
            own.add("", doc);
        }
        Cow::Owned(own)
    }

    /// The vault as `doc` reads it: this one, or, for a document that declares
    /// `::profile(standalone)`, an empty one -- no macros, variables, field types, vocabularies
    /// or dictionaries -- so that what the document means is what its own bytes say.
    ///
    /// Only what changes the meaning is taken away. A constraint the vault puts on a document
    /// (a blueprint, the `type:` a metadata field must have) leaves what it means as it is, so it
    /// is still checked.
    fn seen_by<B>(&self, doc: &File<B>) -> &Vault {
        if reads_alone(doc) {
            self.alone.get_or_init(|| {
                Box::new(Vault::new(
                    PrinterConfig::default(),
                    self.root.clone(),
                    LoadedVocabularies::default(),
                ))
            })
        } else {
            self
        }
    }

    /// This vault with the dictionaries it registered, already loaded.
    pub fn with_dictionaries(mut self, dictionaries: LoadedDictionaries) -> Self {
        self.dictionary_errors = dictionaries.errors.clone();
        self.dictionaries = dictionaries;
        self
    }

    /// The dictionaries of terms this vault registered.
    pub fn dictionaries(&self) -> &LoadedDictionaries {
        &self.dictionaries
    }

    /// Everything wrong with the vault's own setup that no document is to blame for: the
    /// vocabularies and the dictionaries it declared that were not accepted.
    pub fn setup_problems(&self) -> impl Iterator<Item = &String> {
        self.vocabulary_errors
            .iter()
            .chain(self.dictionary_errors.iter())
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
            match tomet_ast_parser::parse_file(src) {
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
    pub fn diagnostics(&self, doc: &File, blueprints: &[File]) -> Vec<Diagnostic> {
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

        if let Some(kind) = tomet_declaration::document_kind(doc)
            && kind != "blueprint"
            && let Some(blueprint) = blueprints.iter().find(|b| {
                tomet_declaration::document_blueprint(b).as_deref() == Some(kind.as_str())
            })
        {
            diagnostics.extend(tomet_validator::validate_against_blueprint_with(
                doc, blueprint, &bindings,
            ));
        }
        diagnostics.extend(self.unresolved_terms(doc));
        diagnostics
    }

    /// The `@term` mentions in `doc` that name no term of this vault's dictionaries. The `@term`
    /// blocks that define the terms of a `::dictionary` document are not mentions.
    fn unresolved_terms(&self, doc: &File) -> Vec<Diagnostic> {
        use tomet_semantics::{Definitions, ElementKind, TermRef, ValueExt};

        let own = tomet_declaration::document_dictionary(doc);
        let dictionaries = self.dictionaries_for(doc);
        let definitions = Definitions::of(doc);
        let mut unresolved = Vec::new();
        tomet_tree::for_each_element(doc, |el| {
            let view = el.as_view();
            if tomet_semantics::classify_std_lenient(view) != ElementKind::Term
                || definitions.contains(el)
            {
                return;
            }
            let Some(target) = tomet_semantics::element_args_view(view).and_then(|args| {
                args.get("target")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            }) else {
                return;
            };
            if let Err(problem) = dictionaries.resolve(&TermRef::parse(&target), own.as_deref()) {
                unresolved.push(Diagnostic::UnresolvedTerm {
                    target,
                    reason: problem.to_string(),
                    span: el.span,
                });
            }
        });
        unresolved
    }

    /// The names in scope for `doc`: `std`, the document's own `::kind`,
    /// and anything it brings in with `::use`.
    ///
    /// This is what makes an element name mean something. Matching on a
    /// bare `Element::name` instead is the shortcut that drops the
    /// namespaced spelling of the same element.
    pub fn bindings<B>(&self, doc: &File<B>) -> Bindings {
        self.seen_by(doc).loaded.bindings_for(doc)
    }

    /// Parses `src` and binds its names against this vault.
    pub fn parse(&self, src: &str) -> Result<(File, Bindings), tomet_ast_parser::Error> {
        let doc = tomet_ast_parser::parse_file(src)?;
        let bindings = self.bindings(&doc);
        Ok((doc, bindings))
    }

    /// Builds the evaluation [`Scope`] for `doc` at `path`, honoring
    /// `::profile(standalone)` by isolating `doc` from the rest of the vault
    /// when requested.
    pub fn scope_for<'a>(
        &'a self,
        doc: &File,
        path: &Path,
        resolver: &'a dyn Resolver,
    ) -> Scope<'a> {
        let (vault, resolver) = self.reading(doc, resolver);
        self.build_scope(&vault, doc, path, resolver)
    }

    /// Builds the evaluation [`Scope`] for `doc` at `path` against the full
    /// vault, ignoring `::profile(standalone)` (used for selective freezing).
    pub fn scope_for_selected<'a>(
        &'a self,
        doc: &File,
        path: &Path,
        resolver: &'a dyn Resolver,
    ) -> Scope<'a> {
        self.build_scope(self, doc, path, resolver)
    }

    fn build_scope<'a>(
        &self,
        vault: &Vault,
        doc: &File,
        path: &Path,
        resolver: &'a dyn Resolver,
    ) -> Scope<'a> {
        let self_doc = self.doc_ref(path);
        let snapshot = Arc::new(doc.clone());
        let config = vault.evaluation_config(doc);
        let vars = vault.vars_in_scope(&config, &self_doc);
        Scope {
            overlay: Overlay {
                meta: typed_meta(&vault.config, &snapshot, &self_doc),
                path: self_doc.clone(),
                snapshot,
                inner: resolver,
            },
            macros: config.macros,
            chain: vec![self_doc.clone()],
            self_doc,
            var: vars.values,
            var_unset: vars.unset,
        }
    }

    /// Gives every `@term` mention that names a term the term's own text, unless the author wrote
    /// something between the brackets, so that a converter renders a word and need not know what a
    /// dictionary is. A mention that names no term is left as it is; `tomet check` reports it.
    pub fn fill_terms(&self, doc: &mut File) {
        use tomet_ast::{Block, Inline, Paragraph, Text};
        use tomet_semantics::{Definitions, ElementKind, TermRef, ValueExt};

        let dictionaries = self.dictionaries_for(doc);
        if dictionaries.is_empty() {
            return;
        }
        let own = tomet_declaration::document_dictionary(doc);
        let definitions = Definitions::of(doc);
        tomet_tree::for_each_element_mut(doc, |el| {
            let view = el.as_view();
            if el
                .content
                .as_ref()
                .is_some_and(|content| !content.is_empty())
                || tomet_semantics::classify_std_lenient(view) != ElementKind::Term
                || definitions.contains(el)
            {
                return;
            }
            let Some(target) = tomet_semantics::element_args_view(view).and_then(|args| {
                args.get("target")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            }) else {
                return;
            };
            if let Ok((_, term)) = dictionaries.resolve(&TermRef::parse(&target), own.as_deref()) {
                el.content = Some(vec![Block::Paragraph(Paragraph::new(
                    vec![Inline::Text(Text::from(term.name.as_str()))],
                    Span::dummy(),
                ))]);
            }
        });
    }

    /// Builds the evaluation [`Scope`] for an included document `doc` in its own context.
    pub fn block_scope<'a>(
        &'a self,
        doc: &DocRef,
        includes: &[DocRef],
        resolver: &'a dyn Resolver,
    ) -> Result<Scope<'a>, String> {
        let source = resolver.document(doc)?;
        let (vault, resolver) = self.reading(&source, resolver);
        let overlay = Overlay {
            meta: typed_meta(&vault.config, &source, doc),
            path: doc.clone(),
            snapshot: source.clone(),
            inner: resolver,
        };
        let config = vault.evaluation_config(&source);
        let vars = vault.vars_in_scope(&config, doc);
        Ok(Scope {
            overlay,
            macros: config.macros,
            chain: includes.to_vec(),
            self_doc: doc.clone(),
            var: vars.values,
            var_unset: vars.unset,
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

    /// The macros and variables in scope for `doc`: its own `::macro` and
    /// `::var`, then the vault's for every name it did not claim.
    ///
    /// The document wins a collision, which is the direction every other
    /// scope in this format runs. It lives here and not in a command, so
    /// that every command sees the same macros.
    fn evaluation_config(&self, doc: &File) -> tomet_semantics::DocumentConfig {
        let mut config = tomet_semantics::document_config(doc);
        for (name, template) in &self.config.macros {
            config
                .macros
                .entry(name.clone())
                .or_insert_with(|| template.clone());
        }
        for (name, decl) in &self.config.vars {
            if !config.vars.iter().any(|(declared, _)| declared == name) {
                config.vars.push((name.clone(), decl.clone()));
            }
        }
        config
    }

    /// `var` when rendering `doc_ref`: each declared variable at its `default`.
    /// Nothing is passed while rendering, so nothing here can be refused.
    fn vars_in_scope(
        &self,
        config: &tomet_semantics::DocumentConfig,
        doc_ref: &DocRef,
    ) -> tomet_expr::VarScope {
        tomet_expr::var_scope(&config.vars, doc_ref, &[])
            .expect("nothing is passed when rendering, so nothing is undeclared or mistyped")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUEPRINT: &str = "::kind(blueprint)\n::blueprint(note)\n::meta(\n  date: \"2026\"\n)\n\n=[ Items ]#(items)::rule(allow: (task))\n";
    const UUID_SETTINGS: &str = "::kind(config)\n::field(id: (type: uuid))\n";

    fn texts(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// The settings a host passes are in force: without them nothing declares
    /// a `type:`, with them a field that has the wrong form is reported.
    #[test]
    fn settings_given_as_text_declare_the_types_of_fields() {
        let source = "::id(not-a-uuid)\n";
        assert!(validate_sources(source, &[], None, &[]).unwrap().is_empty());

        let found = validate_sources(source, &[], Some(UUID_SETTINGS), &[]).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].to_string().contains("not a valid uuid"));
    }

    /// A document is checked against the blueprint for its `::kind`, if the
    /// host passed one, and against none otherwise.
    #[test]
    fn a_blueprint_given_as_text_is_checked_against() {
        let bad = "::kind(note)\n::meta(date: \"2026-10-13\")\n\n=[ Items ]#(items)\n@heading[ Disallowed ]\n";
        let good = "::kind(note)\n::meta(date: \"2026-10-13\")\n\n=[ Items ]#(items)\n@task[ Clean room ]\n";
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
            "::kind(blueprint)\n::blueprint(memo)\n\n=[ Items ]#(items)::rule(allow: (task))\n",
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
            !validate_sources("@nonesuch()\n", &[], None, &[])
                .unwrap()
                .is_empty()
        );
        assert!(
            validate_sources("{$ tmt(\"other.tmt\")::meta.title }\n", &[], None, &[])
                .unwrap()
                .is_empty()
        );
    }

    const ACME: &str = "::kind(vocabulary)\n::vocabulary(acme, version: \"1.0.0\")\n";

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
                "::kind(vocabulary)\n::vocabulary(doc, version: \"1.0.0\")\n",
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

    fn vault_with_dictionary() -> Vault {
        let mut dictionaries = tomet_semantics::LoadedDictionaries::default();
        dictionaries.add(
            "tomet.tmt",
            &tomet_ast_parser::parse_file(
                "::dictionary(tomet)\n\n;@term(\"vault.config.tmt\")#(vault-config)\n| The file that marks a vault.\n",
            )
            .expect("parses"),
        );
        Vault::from_sources(PrinterConfig::default(), ".", &[]).with_dictionaries(dictionaries)
    }

    /// A mention that names no term is an error that says how to fix it; one that does is not.
    #[test]
    fn a_term_mention_is_checked_against_the_registered_dictionaries() {
        let vault = vault_with_dictionary();
        let reasons = |src: &str| -> Vec<String> {
            let (doc, _) = vault.parse(src).expect("parses");
            vault
                .diagnostics(&doc, &[])
                .into_iter()
                .filter_map(|d| match d {
                    Diagnostic::UnresolvedTerm { reason, .. } => Some(reason),
                    _ => None,
                })
                .collect()
        };
        assert!(
            reasons("a @term(tomet#vault-config) b @term(\"vault.config.tmt\") c\n").is_empty()
        );
        assert_eq!(reasons("a @term(#vault-config) b\n").len(), 1);
        assert_eq!(reasons("a @term(tomet#gone) b\n").len(), 1);
        // A definition, a `@term` block of a dictionary's body, is not a mention.
        assert!(reasons("::dictionary(d)\n\n;@term(\"w\")[ W ]\n").is_empty());
    }

    /// A resolved mention comes out as the term's own text, so a converter renders a word; what the
    #[test]
    fn a_vault_in_the_current_directory_names_a_document_without_a_leading_dot() {
        let vault = Vault::from_sources(PrinterConfig::default(), ".", &[]);
        assert_eq!(vault.doc_ref(Path::new("./a.tmt")), DocRef("a.tmt".into()));
        assert_eq!(vault.doc_ref(Path::new("a.tmt")), DocRef("a.tmt".into()));
    }

    const PROMISE: &str = "::profile(standalone)\n";

    fn vault_with_a_macro() -> Vault {
        let config = tomet_config::load_config_from_str(
            "::macro(gh: \"https://gh/$(1)\")\n::field(n: (type: uuid))\n",
        )
        .unwrap();
        Vault::from_sources(config, "/v", &[("acme.tmt", ACME)])
    }

    #[test]
    fn a_standalone_document_is_not_bound_to_the_vaults_vocabulary() {
        let vault = vault_with_a_macro();
        let (_, bound) = vault.parse("::kind(acme)\n").unwrap();
        assert!(bound.kind.is_some());
        let (_, alone) = vault.parse(&format!("{PROMISE}::kind(acme)\n")).unwrap();
        assert!(alone.kind.is_none());
    }

    #[test]
    fn a_standalone_document_does_not_get_the_vaults_dictionaries_but_may_be_one() {
        let vault = vault_with_dictionary();
        let mention = "a @term(tomet#vault-config) b\n";
        assert!(
            vault
                .diagnostics(&vault.parse(&format!("{PROMISE}{mention}")).unwrap().0, &[])
                .iter()
                .any(|d| matches!(d, Diagnostic::UnresolvedTerm { .. }))
        );
        let own = format!("{PROMISE}::dictionary(d)\n\n;@term(\"w\")[ W ]#(w)\n\n@term(d#w)\n");
        assert!(
            !vault
                .diagnostics(&vault.parse(&own).unwrap().0, &[])
                .iter()
                .any(|d| matches!(d, Diagnostic::UnresolvedTerm { .. }))
        );
    }

    /// What a vault puts on a document without changing what it means is still checked.
    #[test]
    fn a_standalone_document_is_still_held_to_a_blueprint() {
        let vault = vault_with_a_macro();
        let blueprint = tomet_ast_parser::parse_file(BLUEPRINT).unwrap();
        let src = format!("{PROMISE}::kind(note)\n\n=[ Other ]\n");
        let doc = vault.parse(&src).unwrap().0;
        let found = vault.diagnostics(&doc, &[blueprint]);
        assert!(
            found
                .iter()
                .any(|d| matches!(d, Diagnostic::MissingRequiredSection { .. })),
            "{found:?}"
        );
    }
}
