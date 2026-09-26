//! Resolves the constructs that name another file: `@config(import:...)`
//! / `@settings(file:...)`, and the vocabularies a document brings into
//! scope. This is Tomet's "preprocessor/linker" layer: the only place I/O
//! and file resolution are allowed, precisely because `tomet-parser`
//! itself must have none (see `crates/tomet-syntax-parser/.writ.tmt`'s
//! `parser-purity`).
//!
//! ## Reading is here; deciding is not
//! Every rule lives one layer down, in `tomet-semantics`, and this crate
//! only adds "read it off disk". `Vocabulary::from_document` and
//! `Bindings::for_document` are pure there; `load_vocabularies` and
//! `bindings_for` are those two plus a `fs::read_to_string`. That split
//! is what lets the js/java/python bindings hand vocabularies in as
//! source text from a target that cannot open a file.
//!
//! ## `@include` is recognized and expands nothing
//! `@import` split into `@use` (bind a namespace, handled here) and
//! `@include` (splice another document in at that point). Only the first
//! is built. `@include` parses and classifies, and no expansion exists
//! for it yet.

mod connect;
mod error;
mod interp;
mod settings;
mod vocabulary;

pub use connect::{RemoteConnection, resolve_connect_targets};
pub use error::ResolveError;
pub use interp::resolve_reference;
pub use settings::{
    config_import_ref, resolve_settings_file, resolve_settings_ref, settings_file_ref,
};
pub use vocabulary::{
    LoadedVocabularies, bindings_for, load_vocabularies, load_vocabulary_sources,
};
