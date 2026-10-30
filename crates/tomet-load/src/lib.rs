//! The one way to read a `.tmt` document correctly.
//!
//! Reading a document is four steps: find the config that governs it
//! (`tomet-load-config`), load the vocabularies that config declares
//! (this crate's own `vocabulary` module), parse the source, and bind
//! the names. A caller that skips the middle two gets a
//! `Document` whose element names resolve against `std` alone, which
//! silently drops every user-vocabulary element. `twrit` shipped exactly
//! that against this workspace's own writs: `@layers` was found and the
//! equally legal `@writ.layers` was not, so the rule it was written to
//! enforce stopped being enforced without anything reporting it.
//!
//! The pieces were public and correct before this crate existed; only the
//! assembly was private, living inside `apps/cli`'s `check` command. That
//! is why this is a crate rather than a function on `tomet-config`: the
//! defect was that nobody could find the front door, and a door inside a
//! crate named after one of its four steps is not findable. `tomet-config`
//! already carried three of the four dependencies, so the cheaper change
//! was available and was not taken for that reason.
//!
//! =[ Two levels, because vocabularies belong to a vault ]
//!
//! [`Vault::discover`] pays for config discovery and vocabulary loading
//! once. [`Vault::document`] is per file. Collapsing the two into a single
//! `load_document` and calling it in a loop would re-read and re-parse
//! every declared vocabulary for every document -- seven vocabularies
//! times eighty-five documents in this repository. [`load_document`] is
//! for the case where there genuinely is one document.
//!
//! =[ A fifth step, for output ]
//!
//! The four steps above produce the document @em[as written]. Something that
//! is about to render it needs one more: every `${...}` in it put back as
//! what it stands for -- text, a list of links, another file's content --
//! before any converter sees the tree, so that a converter never has to
//! decide what an expression means.
//!
//! That step is [`Vault::prepare`], and it is deliberately @strong[not] part of
//! [`Vault::document`]. A caller that wants one file read correctly must not
//! pay for evaluating expressions that may reach other files --
//! `bindings/js` parses a single string in a browser. So the two are
//! separate doors: [`Vault::document`] to read, [`Vault::prepared_document`]
//! to read and prepare.
//!
//! The pairing mirrors the one already here: [`Vault::parse`] is to
//! [`Vault::document`] what [`Vault::prepare`] is to
//! [`Vault::prepared_document`] -- the first of each takes what the caller
//! already has in hand, the second goes to disk for it. A caller that
//! formats its own parse errors (the CLI does) wants `parse` + `prepare`.
//!
//! Preparing asks the vault's files questions -- which documents are under a
//! directory, what a note name stands for, what another document's `::meta`
//! says -- through [`Vault::resolver`], and a document is read at most once
//! however many expressions ask for it.
//!
//! ==[ Who prepares, and who must not ]
//!
//! Anything producing output does: `export`, `html` (and so `serve`),
//! `to-md`, `to-typst`, `to-pandoc`. They read through this crate for that
//! reason -- before it, each parsed on its own and interpolation was
//! decided four different ways, one per converter.
//!
//! The LSP must not. An editor wants the document @em[as written] --
//! rewriting the tree under the cursor would move every span the editor maps
//! positions with. `tomet check` is the middle case -- it prepares a copy to
//! report what could not be resolved, and validates the original -- and that
//! is [`Vault::findings`], which `tomet check`, `tomet stats` and the LSP all
//! ask, so the three say the same thing about a document.
//!
//! An editor holds documents that are not saved. [`Vault::set_buffer`] tells
//! the vault about one, and a document that reads that file -- an `include` of
//! it -- reads the editor's text.

use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use tomet_ast::Document;
use tomet_config::PrinterConfig;
use tomet_semantics::Bindings;
use vocabulary::load_vocabularies;

mod findings;
mod resolver;
mod vocabulary;

pub use findings::{Findings, is_error};
pub use resolver::DiskResolver;
pub use tomet_vault::{Prepared, Unresolved, UnresolvedKind};

/// A vault found on disk: `tomet_vault::Vault` plus the parts that need
/// a filesystem.
///
/// Everything that is only computation -- [`parse`](tomet_vault::Vault::parse),
/// [`bindings`](tomet_vault::Vault::bindings), the `config`/`root` fields --
/// is the inner vault's, reached through `Deref`. What is added here is
/// finding it ([`Vault::discover`]), reading a file in it
/// ([`Vault::document`]), and the vault-wide table an index query is
/// answered from, which is a walk of the disk.
///
/// "Vault" is the directory the config file sits in: `find_config_file`
/// walks up until it finds one, and that file's position is what says
/// where the vault begins.
pub struct Vault {
    core: tomet_vault::Vault,
    /// Documents already read for the evaluator, by vault-relative path.
    /// A `Mutex` so a `Vault` stays `Sync`.
    docs: Mutex<HashMap<String, Arc<Document>>>,
    /// The names of the vault's files, built on the first `ref("note")`.
    links: OnceLock<tomet_links::VaultLinkIndex>,
}

impl Deref for Vault {
    type Target = tomet_vault::Vault;

    fn deref(&self) -> &tomet_vault::Vault {
        &self.core
    }
}

impl Vault {
    /// Finds the vault governing `path` and loads its vocabularies.
    ///
    /// Never fails. With no config file to find, the defaults are used and
    /// `root` is `path`'s directory -- a lone `.tmt` outside any vault is
    /// still readable, it just brings no vocabulary of its own.
    pub fn discover(path: &Path) -> Self {
        let (config, root) = tomet_load_config::find_config_file(path)
            .map(|(config, _, root)| (config, root))
            .unwrap_or_else(|| {
                let root = if path.is_dir() {
                    path.to_path_buf()
                } else {
                    path.parent().unwrap_or(Path::new(".")).to_path_buf()
                };
                (PrinterConfig::default(), root)
            });

        let loaded = load_vocabularies(&root, &config.registry.vocabularies);
        Self {
            core: tomet_vault::Vault::new(config, root, loaded),
            docs: Mutex::new(HashMap::new()),
            links: OnceLock::new(),
        }
    }

    /// The evaluator's view of this vault: what `vault.docs`, `ref("note")`
    /// and `.meta` are answered from.
    pub fn resolver(&self) -> DiskResolver<'_> {
        DiskResolver::new(self)
    }

    pub(crate) fn cached_document(&self, path: &str) -> Option<Arc<Document>> {
        self.docs
            .lock()
            .expect("the document cache is never held across a panic")
            .get(path)
            .cloned()
    }

    pub(crate) fn cache_document(&self, path: &str, doc: Document) -> Arc<Document> {
        let doc = Arc::new(doc);
        self.docs
            .lock()
            .expect("the document cache is never held across a panic")
            .insert(path.to_string(), doc.clone());
        doc
    }

    pub(crate) fn link_index(&self) -> &tomet_links::VaultLinkIndex {
        self.links.get_or_init(|| {
            tomet_links::VaultLinkIndex::from_vault(
                &self.core.root,
                &self.core.config,
                &self.core.root,
            )
        })
    }

    /// Tells the vault that an editor holds `path` open with `text`, which
    /// may not have been saved. A document that asks for that file -- an
    /// `include` of it, a `#id` in it, its `::meta` -- is answered from `text`
    /// from now on, not from the disk.
    ///
    /// Text that does not parse is no buffer at all: the file is read from
    /// the disk again, as if it had never been opened.
    pub fn set_buffer(&self, path: &Path, text: &str) {
        match self.parse(text) {
            Ok((doc, _)) => {
                self.cache_document(&self.doc_ref(path).0, doc);
            }
            Err(_) => self.remove_buffer(path),
        }
    }

    /// Forgets what [`Vault::set_buffer`] was told about `path`: the next
    /// time it is asked for it is read from the disk.
    pub fn remove_buffer(&self, path: &Path) {
        self.docs
            .lock()
            .expect("the document cache is never held across a panic")
            .remove(&self.doc_ref(path).0);
    }

    /// Reads, parses, and binds one file in this vault.
    pub fn document(&self, path: &Path) -> Result<(Document, Bindings), LoadError> {
        let src = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        self.parse(&src).map_err(|source| LoadError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// The fifth step: resolves every `${...}` in `doc`, so it is the shape
    /// a converter should see. What that means is
    /// `tomet_vault::Vault::prepare`'s doc; this supplies the files the
    /// expressions ask about.
    pub fn prepare(&self, doc: &mut Document, path: &Path) -> Prepared {
        self.core.prepare(doc, path, &self.resolver())
    }

    /// What `expr`, written in `doc` at `path`, comes to when the document is
    /// rendered -- see `tomet_vault::Vault::evaluate`. This supplies the
    /// files the expression asks about, and the editor's unsaved text for the
    /// ones it has open.
    pub fn evaluate(
        &self,
        doc: &Document,
        path: &Path,
        expr: &tomet_ast::InterpExpr,
    ) -> Result<tomet_expr::EvalValue, tomet_expr::EvalError> {
        self.core.evaluate(doc, path, expr, &self.resolver())
    }

    /// [`Vault::document`] followed by [`Vault::prepare`] -- what anything
    /// about to render a document wants.
    pub fn prepared_document(&self, path: &Path) -> Result<(Document, Bindings), LoadError> {
        let (mut doc, bindings) = self.document(path)?;
        self.prepare(&mut doc, path);
        Ok((doc, bindings))
    }
}

/// Reads one document correctly, discovering its vault first.
///
/// For more than one document in the same vault, use [`Vault::discover`]
/// once and [`Vault::document`] per file: this function reloads every
/// declared vocabulary on each call.
pub fn load_document(path: &Path) -> Result<(Document, Bindings), LoadError> {
    Vault::discover(path).document(path)
}

/// Why a document could not be read.
#[derive(Debug)]
pub enum LoadError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: tomet_parser::Error,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // The cause is not interpolated here: it is what `source`
            // returns, and `anyhow`'s `{:#}` walks that chain. Saying it
            // in both places prints it twice.
            LoadError::Io { path, .. } => write!(f, "failed to read {}", path.display()),
            LoadError::Parse { path, .. } => write!(f, "failed to parse {}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Io { source, .. } => Some(source),
            LoadError::Parse { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tomet_ast::{Block, Inline};

    /// The text of every paragraph and the target of every link, in order.
    fn flat(doc: &Document) -> Vec<String> {
        fn inlines(out: &mut Vec<String>, items: &[Inline]) {
            for inline in items {
                match inline {
                    Inline::Text(t) => out.push(t.value.clone()),
                    Inline::Element(el) => {
                        if let Some((_, target)) = tomet_semantics::link_target_of(el) {
                            out.push(format!("link:{target}"));
                        }
                    }
                    _ => {}
                }
            }
        }
        fn blocks(out: &mut Vec<String>, items: &[Block]) {
            for block in items {
                match block {
                    Block::Paragraph(p) => inlines(out, &p.content),
                    Block::List(list) => {
                        for item in &list.items {
                            if let Some(content) = &item.element.content {
                                blocks(out, content);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        blocks(&mut out, &doc.blocks);
        out
    }

    fn vault_fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("notes")).expect("temp dirs");
        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::settings{ meta: { tags: { type: list }, created: { type: datetime } } }\n",
        )
        .expect("config");
        for (file, tags, created) in [
            ("rust.tmt", "list(rust)", "2026-01-01"),
            ("cli.tmt", "list(rust, cli)", "2026-05-01"),
            ("go.tmt", "list(go)", "2026-09-01"),
        ] {
            fs::write(
                root.join("notes").join(file),
                format!("::meta{{ tags: {tags}, created: \"{created}\" }}\n\n=[ Note ]\n"),
            )
            .expect("note");
        }
        root
    }

    const QUERY: &str = "- ${vault.docs\n    .filter(d => d.meta.tags.contains(\"rust\"))\n    .sort(d => d.meta.created, \"desc\")\n    .map(d => d.link())}\n";

    #[test]
    fn prepared_document_answers_a_query_from_the_vault() {
        let root = vault_fixture("tomet_test_load_prepared");
        fs::write(root.join("index.tmt"), QUERY).expect("index");

        let vault = Vault::discover(&root);
        let (doc, _bindings) = vault
            .prepared_document(&root.join("index.tmt"))
            .expect("prepared");
        assert_eq!(
            flat(&doc),
            ["link:tmt:notes/cli.tmt", "link:tmt:notes/rust.tmt"]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// The reason `prepare` is a separate door from `document`: a caller
    /// that only wants one file read gets it as written, expressions and all.
    #[test]
    fn reading_a_document_does_not_evaluate_it() {
        let root = vault_fixture("tomet_test_load_as_written");
        fs::write(root.join("plain.tmt"), "sum ${add(1, 2)}\n").expect("plain");

        let vault = Vault::discover(&root);
        let (doc, _) = vault.document(&root.join("plain.tmt")).expect("read");
        assert!(
            flat(&doc).iter().all(|t| !t.contains('3')),
            "{:?}",
            flat(&doc)
        );
        let (prepared, _) = vault
            .prepared_document(&root.join("plain.tmt"))
            .expect("prepared");
        assert_eq!(flat(&prepared), ["sum ", "3"]);

        let _ = fs::remove_dir_all(&root);
    }

    /// A document is read once, however many expressions and documents ask.
    #[test]
    fn a_document_is_read_once_across_documents_in_one_vault() {
        let root = vault_fixture("tomet_test_load_shared_docs");
        for name in ["a.tmt", "b.tmt"] {
            fs::write(root.join(name), QUERY).expect("index");
        }

        let vault = Vault::discover(&root);
        let (first, _) = vault.prepared_document(&root.join("a.tmt")).expect("a");
        let cached = vault
            .cached_document("notes/go.tmt")
            .expect("read by the first query");
        let (second, _) = vault.prepared_document(&root.join("b.tmt")).expect("b");

        assert_eq!(flat(&first), flat(&second));
        assert!(
            Arc::ptr_eq(&cached, &vault.cached_document("notes/go.tmt").unwrap()),
            "the document was read again for the second one"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_expression_that_cannot_be_answered_stays_in_the_text_and_is_reported() {
        let root = vault_fixture("tomet_test_load_bad_expression");
        fs::write(
            root.join("index.tmt"),
            "${vault.docs.filter(d => d.meta.tgs.contains(\"x\"))}\n",
        )
        .expect("index");

        let vault = Vault::discover(&root);
        let (mut doc, _) = vault.document(&root.join("index.tmt")).expect("read");
        let prepared = vault.prepare(&mut doc, &root.join("index.tmt"));
        assert_eq!(prepared.unresolved.len(), 1);
        assert!(
            prepared.unresolved[0].reason.contains("tgs"),
            "{}",
            prepared.unresolved[0].reason
        );
        assert!(
            flat(&doc)[0].starts_with("${vault.docs"),
            "{:?}",
            flat(&doc)
        );

        let _ = fs::remove_dir_all(&root);
    }

    // ---- include -------------------------------------------------------------

    /// `level:title` for each section and `text` for each paragraph, in order.
    fn outline(doc: &Document) -> Vec<String> {
        fn text(inlines: &[Inline]) -> String {
            inlines
                .iter()
                .filter_map(|i| match i {
                    Inline::Text(t) => Some(t.value.as_str()),
                    _ => None,
                })
                .collect()
        }
        fn walk(out: &mut Vec<String>, blocks: &[Block]) {
            for block in blocks {
                match block {
                    Block::Section(s) => {
                        out.push(format!("{}:{}", s.level, text(&s.title).trim()));
                        walk(out, &s.blocks);
                    }
                    Block::Paragraph(p) => out.push(text(&p.content).trim().to_string()),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&mut out, &doc.blocks);
        out
    }

    fn include_vault(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp dir");
        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("config");
        for (path, source) in files {
            let full = root.join(path);
            fs::create_dir_all(full.parent().unwrap()).expect("dirs");
            fs::write(full, source).expect("file");
        }
        root
    }

    #[test]
    fn a_setting_only_the_vault_makes_is_reported_anywhere_else() {
        let root = include_vault(
            "tomet_test_load_vault_only",
            &[
                (
                    "a/.config.tmt",
                    "::kind(config)\n::config{ workspace: { ignore: list(\"x\") } }\n",
                ),
                (
                    "note.tmt",
                    "::config{ workspace: { unswept: list(\"y\") } }\n\nbody\n",
                ),
            ],
        );
        let vault = Vault::discover(&root);
        let findings_of = |path: &std::path::Path| {
            let doc = tomet_parser::parse_document(&fs::read_to_string(path).unwrap()).unwrap();
            vault.findings(path, &doc)
        };
        let reported = |findings: &Findings| {
            findings.diagnostics.iter().any(|d| {
                matches!(d, tomet_validator::Diagnostic::VaultOnlySetting { key, .. } if key == "workspace")
            })
        };

        for rel in ["a/.config.tmt", "note.tmt"] {
            let findings = findings_of(&root.join(rel));
            assert!(reported(&findings), "{rel}: {:?}", findings.diagnostics);
            assert!(findings.has_errors(), "{rel}");
        }

        // A config document that is no layer of the stack is a sample: nothing reads it.
        let sample = root.join("sample.tmt");
        fs::write(
            &sample,
            "::kind(config)\n::config{ workspace: { ignore: list(\"x\") } }\n",
        )
        .unwrap();
        assert!(!reported(&findings_of(&sample)));

        // The vault config is the one place it belongs.
        let vault_file = root.join(tomet_load_config::VAULT_CONFIG_FILE);
        fs::write(
            &vault_file,
            "::config{ workspace: { ignore: list(\"x\") } }\n",
        )
        .unwrap();
        assert!(!reported(&findings_of(&vault_file)));
    }

    #[test]
    fn an_included_file_is_prepared_in_its_own_place_and_lands_under_the_heading() {
        let root = include_vault(
            "tomet_test_load_include_own",
            &[
                (
                    "host.tmt",
                    "=[ Host ]\n\nhere is ${self.path}\n\n${include(tmt(\"parts/a.tmt\"))}\n",
                ),
                (
                    "parts/a.tmt",
                    "::meta{ title: A }\n\n=[ A ]\n\nI am ${self.path} (${self.meta.title})\n",
                ),
            ],
        );
        let vault = Vault::discover(&root);
        let (doc, _) = vault
            .prepared_document(&root.join("host.tmt"))
            .expect("host");
        assert_eq!(
            outline(&doc),
            [
                "1:Host",
                "here is host.tmt",
                // `self` in the included file is the included file, and the
                // heading is one below the one it landed under.
                "2:A",
                "I am parts/a.tmt (A)",
            ]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn includes_nest_and_each_level_shifts_what_is_inside_it() {
        let root = include_vault(
            "tomet_test_load_include_nest",
            &[
                ("top.tmt", "=[ Top ]\n\n${include(tmt(\"mid.tmt\"))}\n"),
                ("mid.tmt", "=[ Mid ]\n\n${include(tmt(\"leaf.tmt\"))}\n"),
                ("leaf.tmt", "=[ Leaf ]\n\nleaf text\n"),
            ],
        );
        let vault = Vault::discover(&root);
        let (doc, _) = vault.prepared_document(&root.join("top.tmt")).expect("top");
        assert_eq!(outline(&doc), ["1:Top", "2:Mid", "3:Leaf", "leaf text"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn two_files_that_include_each_other_are_reported_and_left_as_written() {
        let root = include_vault(
            "tomet_test_load_include_cycle",
            &[
                ("a.tmt", "=[ A ]\n\n${include(tmt(\"b.tmt\"))}\n"),
                ("b.tmt", "=[ B ]\n\n${include(tmt(\"a.tmt\"))}\n"),
            ],
        );
        let vault = Vault::discover(&root);
        let (mut doc, _) = vault.document(&root.join("a.tmt")).expect("a");
        let prepared = vault.prepare(&mut doc, &root.join("a.tmt"));
        // `a` includes `b`, and `b` includes `a` back: that last include is
        // the cycle, and it stays as written inside `b`. `a` itself, which
        // only includes `b`, resolved -- and says that what it brought in
        // from `b` has one thing left unresolved, which `b` reports in full.
        assert_eq!(outline(&doc), ["1:A", "2:B", "${include(tmt(\"a.tmt\"))}"]);
        assert_eq!(prepared.unresolved.len(), 1, "{:?}", prepared.unresolved);
        assert!(
            prepared.unresolved[0]
                .reason
                .contains("`b.tmt` has 1 unresolved item"),
            "{:?}",
            prepared.unresolved
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// An editor's unsaved text stands in for the file: the including
    /// document is prepared from it, not from what is on the disk.
    #[test]
    fn an_open_buffer_is_what_an_include_reads() {
        let root = include_vault(
            "tomet_test_load_include_buffer",
            &[
                ("host.tmt", "=[ Host ]\n\n${include(tmt(\"lib.tmt\"))}\n"),
                ("lib.tmt", "=[ Saved ]\n"),
            ],
        );
        let vault = Vault::discover(&root);
        let prepared = |vault: &Vault| {
            let (doc, _) = vault.prepared_document(&root.join("host.tmt")).unwrap();
            outline(&doc)
        };
        assert_eq!(prepared(&vault), ["1:Host", "2:Saved"]);

        vault.set_buffer(&root.join("lib.tmt"), "=[ Edited ]\n");
        assert_eq!(prepared(&vault), ["1:Host", "2:Edited"]);

        // Text that does not parse is no buffer: the disk is read again.
        vault.set_buffer(&root.join("lib.tmt"), "@caution[ unterminated\n");
        assert_eq!(prepared(&vault), ["1:Host", "2:Saved"]);

        vault.set_buffer(&root.join("lib.tmt"), "=[ Edited ]\n");
        vault.remove_buffer(&root.join("lib.tmt"));
        assert_eq!(prepared(&vault), ["1:Host", "2:Saved"]);
        let _ = fs::remove_dir_all(&root);
    }

    /// An expression is evaluated the way rendering evaluates it: in the
    /// vault, so it can ask about other documents, and about an open buffer
    /// as the editor has it.
    #[test]
    fn an_expression_is_evaluated_in_the_vault() {
        let root = include_vault(
            "tomet_test_load_evaluate",
            &[
                ("host.tmt", "::meta{ title: Host }\n\n=[ Intro ]#(intro)\n"),
                ("other.tmt", "::meta{ title: Saved }\n"),
            ],
        );
        let vault = Vault::discover(&root);
        let host = root.join("host.tmt");
        let (doc, _) = vault.document(&host).unwrap();
        let eval = |src: &str| {
            let expr = match tomet_parser::parse_dollar_element_str(&format!("${{{src}}}"))
                .unwrap()
                .value
            {
                Some(tomet_ast::ElementValue::Interp(expr)) => expr,
                other => panic!("not an expression: {other:?}"),
            };
            vault.evaluate(&doc, &host, &expr)
        };
        let text = |s: &str| tomet_expr::EvalValue::String(s.to_string());

        assert_eq!(eval("self.meta.title").unwrap(), text("Host"));
        assert_eq!(
            eval("tmt(\"other.tmt\").meta.title").unwrap(),
            text("Saved")
        );
        assert_eq!(eval("#intro.id").unwrap(), text("intro"));

        // The other document is open in the editor, edited and not saved.
        vault.set_buffer(&root.join("other.tmt"), "::meta{ title: Edited }\n");
        assert_eq!(
            eval("tmt(\"other.tmt\").meta.title").unwrap(),
            text("Edited")
        );

        // As when rendering, what exists only when creating is not there.
        assert!(eval("generate.uuid()").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_section_is_included_by_its_id() {
        let root = include_vault(
            "tomet_test_load_include_section",
            &[
                (
                    "host.tmt",
                    "=[ Host ]\n\n${include(tmt(\"lib.tmt\")#shared, heading: false)}\n",
                ),
                (
                    "lib.tmt",
                    "=[ Other ]\n\nnot this\n\n=[ Shared ]#(shared)\n\nuse this\n\n==[ Deep ]\n\nand this\n",
                ),
            ],
        );
        let vault = Vault::discover(&root);
        let (doc, _) = vault
            .prepared_document(&root.join("host.tmt"))
            .expect("host");
        assert_eq!(outline(&doc), ["1:Host", "use this", "2:Deep", "and this"]);
        let _ = fs::remove_dir_all(&root);
    }
}
