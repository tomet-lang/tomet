//! =[ tomet-address ]
//!
//! Resolves what an expression or element points at, without ever
//! touching disk: a `${...}` interpolation's `Identifier`/`Member`
//! chain against the same document's `#(id)`-tagged nodes
//! ([`resolve_reference`]), and recognizing -- not reading -- a
//! `@config(import:...)`/`@settings(file:...)` reference to another
//! file ([`config_import_ref`]/[`settings_file_ref`]).
//!
//! Split out of what used to be `tomet-resolver`'s pure half: that
//! crate's whole remaining point was reading files off disk, so once
//! its fs-touching functions moved to `tomet-load`/`tomet-load-config`
//! and its other pure leftover (`load_vocabulary_sources`) turned out
//! to need no separate home at all, nothing stayed behind that was
//! actually about *resolving a reference* -- this crate is that,
//! named for what it does rather than inheriting a name whose point
//! (I/O) had moved elsewhere.
//!
//! Actually resolving a `@config(import:...)` reference -- reading and
//! merging the target -- lives in `apps/lsp`, which reads it itself
//! rather than calling back in here.

mod error;
mod import_ref;
mod reference;

pub use error::ResolveError;
pub use import_ref::{config_import_ref, settings_file_ref};
pub use reference::resolve_reference;
