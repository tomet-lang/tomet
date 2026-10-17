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
//! directory, what a note name stands for, what another document's `@meta`
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
//! The LSP must not, and is deliberately left out. An editor wants the
//! document @em[as written] -- rewriting the tree under the cursor would move
//! every span the editor maps positions with. `tomet check` is the middle
//! case -- it prepares a copy to report what could not be resolved, and
//! validates the original.

use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use tomet_ast::Document;
use tomet_config::PrinterConfig;
use tomet_semantics::Bindings;
use vocabulary::load_vocabularies;

mod resolver;
mod vocabulary;

pub use resolver::DiskResolver;
pub use tomet_vault::{Prepared, Unresolved};

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
            root.join("default.config.tmt"),
            "@kind(config)\n@settings{ meta: { tags: { type: list }, created: { type: datetime } } }\n",
        )
        .expect("config");
        for (file, tags, created) in [
            ("rust.tmt", "list(rust)", "2026-01-01"),
            ("cli.tmt", "list(rust, cli)", "2026-05-01"),
            ("go.tmt", "list(go)", "2026-09-01"),
        ] {
            fs::write(
                root.join("notes").join(file),
                format!("@meta{{ tags: {tags}, created: \"{created}\" }}\n\n=[ Note ]\n"),
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
}
