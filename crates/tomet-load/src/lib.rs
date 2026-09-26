//! The one way to read a `.tmt` document correctly.
//!
//! Reading a document is four steps across three crates: find the config
//! that governs it, load the vocabularies that config declares, parse the
//! source, and bind the names. A caller that skips the middle two gets a
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
//! # Two levels, because vocabularies belong to a vault
//!
//! [`Vault::discover`] pays for config discovery and vocabulary loading
//! once. [`Vault::document`] is per file. Collapsing the two into a single
//! `load_document` and calling it in a loop would re-read and re-parse
//! every declared vocabulary for every document -- seven vocabularies
//! times eighty-five documents in this repository. [`load_document`] is
//! for the case where there genuinely is one document.
//!
//! # A fifth step, for output
//!
//! The four steps above produce the document *as written*. Something that
//! is about to render it needs one more: the AST rewrites that have to
//! happen before any converter sees the tree. An index document's
//! `${filter(...)}` is one -- it expands into several `@link(ref:...)`
//! entries, so it cannot be a renderer's job the way `${gh(12)}` can.
//!
//! That step is [`Vault::prepare`], and it is deliberately **not** part of
//! [`Vault::document`]. It reads every `.tmt` in the vault, and a caller
//! that wants one file read correctly must not pay for a directory walk --
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
//! ## Who prepares, and who must not
//!
//! Anything producing output does: `export`, `html` (and so `serve`),
//! `to-md`, `to-typst`, `to-pandoc`. They read through this crate for that
//! reason -- before it, each parsed on its own and interpolation was
//! decided four different ways, one per converter.
//!
//! The LSP must not, and is deliberately left out. An editor wants the
//! document *as written*: rewriting the tree under the cursor would move
//! every span the editor maps positions with. `tomet check` is the middle
//! case -- it prepares a copy to report what could not be resolved, and
//! validates the original.

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tomet_ast::Document;
use tomet_config::PrinterConfig;
use tomet_resolver::load_vocabularies;
use tomet_semantics::Bindings;

mod index;

pub use index::{IndexQueryError, VaultIndex, is_index_document};
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
    /// Filled by the first [`Vault::prepare`] that meets an
    /// `@kind(doc.index)` document, and shared by every one after it. A
    /// directory export would otherwise re-read the whole vault per file.
    ///
    /// `OnceLock` rather than `OnceCell` so a `Vault` stays `Sync` and can
    /// be shared rather than rebuilt -- a server answering requests from
    /// one vault is the obvious caller, and `serve` builds a fresh one per
    /// request today only because nothing made it cheap to keep.
    index: OnceLock<VaultIndex>,
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
        let (config, root) = tomet_config::find_config_file(path)
            .map(|(config, _, root)| (config, root))
            .unwrap_or_else(|| {
                let root = if path.is_dir() {
                    path.to_path_buf()
                } else {
                    path.parent().unwrap_or(Path::new(".")).to_path_buf()
                };
                (PrinterConfig::default(), root)
            });

        let loaded = load_vocabularies(&root, &config.vocabularies);
        Self {
            core: tomet_vault::Vault::new(config, root, loaded),
            index: OnceLock::new(),
        }
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

    /// The fifth step: rewrites `doc` into the shape a converter should
    /// see. What the rewrites are is `tomet_vault::Vault::prepare`'s doc;
    /// this supplies the vault-wide table it needs, building it on the
    /// first `@kind(doc.index)` document and reusing it for every one
    /// after, so a document that isn't one costs a single `document_kind`
    /// read and no I/O.
    pub fn prepare(&self, doc: &mut Document, path: &Path) -> Result<Prepared, IndexQueryError> {
        self.core.prepare(doc, path, || {
            self.index
                .get_or_init(|| VaultIndex::build(&self.core.root))
                .rows()
        })
    }

    /// [`Vault::document`] followed by [`Vault::prepare`] -- what anything
    /// about to render a document wants.
    pub fn prepared_document(&self, path: &Path) -> Result<(Document, Bindings), LoadError> {
        let (mut doc, bindings) = self.document(path)?;
        self.prepare(&mut doc, path)
            .map_err(|source| LoadError::Prepare {
                path: path.to_path_buf(),
                source,
            })?;
        Ok((doc, bindings))
    }

    /// The vault's metadata table, if some document has already needed it.
    ///
    /// `None` means no query has been met yet, not that the vault is
    /// empty -- the table is never built speculatively.
    pub fn index(&self) -> Option<&VaultIndex> {
        self.index.get()
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
    /// The document read, and then failed the fifth step -- a query it
    /// carries could not be answered.
    Prepare {
        path: PathBuf,
        source: IndexQueryError,
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
            LoadError::Prepare { path, .. } => {
                write!(f, "failed to prepare {}", path.display())
            }
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::Io { source, .. } => Some(source),
            LoadError::Parse { source, .. } => Some(source),
            LoadError::Prepare { source, .. } => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tomet_ast::Block;

    /// The `ref:` path of every block-level `@link`, in order, with the
    /// scheme prefix stripped.
    fn file_paths(doc: &Document) -> Vec<String> {
        doc.blocks
            .iter()
            .filter_map(|block| match block {
                Block::Element(el) => {
                    let (kind, target) = tomet_semantics::link_target_of(el)?;
                    if kind != tomet_semantics::ElementKind::Link {
                        return None;
                    }
                    let (scheme, rest) = tomet_semantics::target_scheme(&target);
                    (scheme == tomet_semantics::TargetScheme::Ref).then(|| rest.to_string())
                }
                _ => None,
            })
            .collect()
    }

    fn vault_fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("notes")).expect("temp dirs");
        fs::write(root.join("default.config.tmt"), "@kind(config)\n").expect("config");
        for (file, tags, created) in [
            ("rust.tmt", "list(rust)", "2026-01-01"),
            ("cli.tmt", "list(rust, cli)", "2026-05-01"),
            ("go.tmt", "list(go)", "2026-09-01"),
        ] {
            fs::write(
                root.join("notes").join(file),
                format!("@meta{{ tags: {tags}, created: \"{created}\" }}\n\n#[ Note ]\n"),
            )
            .expect("note");
        }
        root
    }

    #[test]
    fn prepared_document_answers_a_query_from_the_vault() {
        let root = vault_fixture("tomet_test_load_prepared");
        fs::write(
            root.join("index.tmt"),
            "@kind(doc.index)\n\n${filter(contains(meta.tags, \"rust\"), by(meta.created, \"desc\"))}\n",
        )
        .expect("index");

        let vault = Vault::discover(&root);
        let (doc, _bindings) = vault
            .prepared_document(&root.join("index.tmt"))
            .expect("prepared");
        assert_eq!(file_paths(&doc), ["notes/cli.tmt", "notes/rust.tmt"]);

        let _ = fs::remove_dir_all(&root);
    }

    /// The reason `prepare` is a separate door from `document`: a caller
    /// that only wants one file read must not pay for a walk of the vault.
    #[test]
    fn a_document_with_no_query_never_builds_the_table() {
        let root = vault_fixture("tomet_test_load_no_query");
        fs::write(root.join("plain.tmt"), "@kind(writ)\n\n#[ Plain ]\n").expect("plain");

        let vault = Vault::discover(&root);
        assert!(vault.index().is_none());

        let (mut doc, _) = vault.document(&root.join("plain.tmt")).expect("read");
        assert_eq!(
            vault
                .prepare(&mut doc, &root.join("plain.tmt"))
                .expect("prepare")
                .expanded_queries,
            0
        );
        assert!(
            vault.index().is_none(),
            "the table was built for a document that asked nothing"
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// And it is built once, not once per document.
    #[test]
    fn the_table_is_shared_across_documents_in_one_vault() {
        let root = vault_fixture("tomet_test_load_shared_table");
        for name in ["a.tmt", "b.tmt"] {
            fs::write(
                root.join(name),
                "@kind(doc.index)\n\n${filter(contains(meta.tags, \"go\"))}\n",
            )
            .expect("index");
        }

        let vault = Vault::discover(&root);
        let first = vault.prepared_document(&root.join("a.tmt")).expect("a");
        let built = vault.index().expect("built by the first query") as *const VaultIndex;
        let second = vault.prepared_document(&root.join("b.tmt")).expect("b");
        let reused = vault.index().expect("still there") as *const VaultIndex;

        assert_eq!(file_paths(&first.0), ["notes/go.tmt"]);
        assert_eq!(file_paths(&second.0), ["notes/go.tmt"]);
        assert_eq!(
            built, reused,
            "the table was rebuilt for the second document"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_query_that_cannot_be_answered_is_a_load_error() {
        let root = vault_fixture("tomet_test_load_bad_query");
        fs::write(
            root.join("index.tmt"),
            "@kind(doc.index)\n\n${filter(exists(meta.tgs))}\n",
        )
        .expect("index");

        let vault = Vault::discover(&root);
        let err = vault
            .prepared_document(&root.join("index.tmt"))
            .expect_err("a misspelt field cannot be answered");
        assert!(matches!(err, LoadError::Prepare { .. }), "{err:?}");

        let _ = fs::remove_dir_all(&root);
    }
}
