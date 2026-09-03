//! `tomet_ast::Document` <-> Pandoc's AST, over Pandoc's own JSON
//! encoding.
//!
//! Speaking that JSON is all it takes to reach every format Pandoc reads
//! or writes, in both directions, without adding anything to Pandoc
//! itself:
//!
//! ```text
//! tomet to-pandoc doc.tmt | pandoc -f json -t docx -o doc.docx
//! pandoc -t json report.docx | tomet from-pandoc
//! ```
//!
//! The lossy edge is Pandoc's `Attr`, whose values are plain strings: a
//! nested Tomet `{value}` has no direct representation there. That is
//! handled by `tomet_semantics::flatten`, whose rule the HTML writer
//! shares -- a readable projection plus an exact JSON copy whenever the
//! projection would lose something.
//!
//! # Why a JSON bridge
//!
//! Two alternatives were considered and rejected.
//!
//! **Upstreaming a format into `jgm/pandoc`** means Haskell, maintainer
//! buy-in, and a user base Tomet does not have -- and it would put
//! Tomet's toolchain inside someone else's release cycle. Pandoc ships
//! Lua custom readers and writers precisely so formats do not need
//! upstreaming.
//!
//! **A Lua custom reader** means a *second* Tomet parser, in Lua. The
//! repository already pays for two grammars (`tomet-parser` and
//! `tree-sitter-tomet`, whose drift `tomet-tests` has to police); a third
//! implementation is not worth the `pandoc -f tomet` spelling. If that
//! spelling is wanted later, a thin Lua wrapper can shell out to the CLI
//! via `pandoc.pipe`.
//!
//! Note also what this crate does *not* buy: `tomet export -t commonmark
//! | pandoc -f markdown -t docx` already reaches every Pandoc format with
//! no code at all. What is gained here is fidelity -- element data
//! reaching `Attr` instead of being dropped at the CommonMark boundary --
//! and a better import.

pub mod ast;
mod from_pandoc;
mod to_pandoc;

pub use ast::{Attr, Block, Inline, Meta, MetaValue, PANDOC_API_VERSION, PandocDoc, Target};
pub use from_pandoc::from_pandoc;
pub use to_pandoc::to_pandoc;
