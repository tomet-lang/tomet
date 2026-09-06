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

use std::path::{Path, PathBuf};

use tomet_ast::Document;
use tomet_config::PrinterConfig;
use tomet_resolver::{LoadedVocabularies, bindings_for, load_vocabularies};
use tomet_semantics::Bindings;

/// A vault's config and the vocabularies it declares, resolved once.
///
/// "Vault" is the directory the config file sits in: `find_config_file`
/// walks up until it finds one, and that file's position is what says
/// where the vault begins.
pub struct Vault {
    /// The config governing this vault, or the defaults when no config
    /// file was found.
    pub config: PrinterConfig,
    /// The directory the config file was found in, or the starting
    /// directory when there was none. Declared vocabulary paths are
    /// relative to this.
    pub root: PathBuf,
    /// Vocabularies that were declared but could not be read. Not fatal:
    /// a caller reports them and carries on, because one unreadable
    /// vocabulary should not hide the state of every document.
    pub vocabulary_errors: Vec<String>,
    loaded: LoadedVocabularies,
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
            config,
            root,
            vocabulary_errors: loaded.errors.clone(),
            loaded,
        }
    }

    /// The names in scope for `doc`: `std`, the document's own `@kind`,
    /// and anything it brings in with `@use`.
    ///
    /// This is what makes an element name mean something. Matching on a
    /// bare `Sigil::Named` instead is the shortcut that drops the
    /// namespaced spelling of the same element.
    pub fn bindings(&self, doc: &Document) -> Bindings {
        bindings_for(doc, &self.loaded)
    }

    /// Parses `src` and binds its names against this vault.
    pub fn parse(&self, src: &str) -> Result<(Document, Bindings), tomet_parser::Error> {
        let doc = tomet_parser::parse_document(src)?;
        let bindings = self.bindings(&doc);
        Ok((doc, bindings))
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
            LoadError::Io { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            LoadError::Parse { path, source } => {
                write!(f, "failed to parse {}: {source}", path.display())
            }
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
