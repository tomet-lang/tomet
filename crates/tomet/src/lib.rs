//! The crate to depend on when using Tomet from Rust.
//!
//! This workspace is split into a couple of dozen crates because the layers
//! it is built from (see `crate-layering` in the root `.writ.tmt`) are worth
//! keeping apart. A consumer should not have to know that. This crate only
//! re-exports: it holds no logic, and each item below is defined where the
//! crate docs of its home say it is.
//!
//! `apps/*` do not use it. The CLI and LSP need the parts a consumer should
//! not: the CST, the lexer, the resolver, the workspace and the TUI. Routing
//! them through here would turn this crate into a mirror of the workspace.
//! The language bindings (`bindings/python`, `java`, `js`) do use it, which
//! is what keeps this surface honest -- what a binding still takes directly
//! (`js`: `tomet-links`, `tomet-transform`, `tomet-lexer`) is what is
//! missing here.
//!
//! # Reading a document
//!
//! Go through [`Vault`]. It finds the config that governs the file, loads
//! the vocabularies that config declares, parses, and binds names -- the
//! four steps that have to happen together. The bare parser is deliberately
//! not re-exported: a caller that parses on its own gets a [`Document`]
//! whose user-vocabulary elements silently do not resolve.
//!
//! ```no_run
//! use std::path::Path;
//!
//! let (doc, _bindings) = tomet::load_document(Path::new("docs/intro.tmt"))?;
//! # let _ = doc;
//! # Ok::<(), tomet::LoadError>(())
//! ```
//!
//! A caller with no filesystem -- a wasm host, or a binding handed
//! vocabulary text -- builds the disk-free [`vault::Vault`] with
//! `from_sources` instead. Both go through the same rules for what a
//! vocabulary may claim, so they give the same verdicts.
//!
//! [`parse_value`] is the one bare parse that is exported. It reads a
//! data-only document -- `key: value` entries, no headings, prose or
//! top-level elements -- so there is no vocabulary for it to skip.
//!
//! # Features
//!
//! [`semantics`] and [`validator`] are always present: they are pure, and
//! reading a document already depends on the first. Everything that leaves
//! the AST is opt-in, so a reader-only consumer
//! compiles nothing it does not use. Each feature adds one module:
//!
//! | feature    | module            | from                 |
//! |------------|-------------------|----------------------|
//! | `html`     | [`html`]          | `tomet-html`         |
//! | `markdown` | [`markdown`]      | `tomet-markdown`     |
//! | `typst`    | [`typst`]         | `tomet-typst`        |
//! | `pandoc`   | [`pandoc`]        | `tomet-pandoc`       |
//! | `format`   | [`mod@format`]    | `tomet-formatter`    |
//! | `printer`  | [`printer`]       | `tomet-printer`      |
//!
//! Output-producing conversions want a document that has been through
//! [`Vault::prepare`] first; see `tomet-load` for why that is a separate
//! step from reading.

/// The syntax tree: [`Document`], [`ast::Element`], [`ast::Value`], ...
pub use tomet_ast as ast;
pub use tomet_ast::Document;
/// `PrinterConfig` and how a vault's config is found and read.
pub use tomet_config as config;
/// What an element means: vocabularies, bindings, built-in classification.
pub use tomet_semantics as semantics;
/// Diagnostics for a parsed document.
pub use tomet_validator as validator;

pub use tomet_load::{
    IndexQueryError, LoadError, Prepared, Unresolved, Vault, VaultIndex, load_document,
};
/// Parses a data-only document into a [`ast::Value`]. Not a way to read a
/// document; use [`Vault`] for that.
pub use tomet_parser::parse_value;

/// The disk-free `Vault`: build one from text with `from_sources`. The
/// top-level [`Vault`] is this plus finding and reading files.
pub use tomet_vault as vault;

#[cfg(feature = "format")]
pub use tomet_formatter as format;
#[cfg(feature = "html")]
pub use tomet_html as html;
#[cfg(feature = "markdown")]
pub use tomet_markdown as markdown;
#[cfg(feature = "pandoc")]
pub use tomet_pandoc as pandoc;
#[cfg(feature = "printer")]
pub use tomet_printer as printer;
#[cfg(feature = "typst")]
pub use tomet_typst as typst;
