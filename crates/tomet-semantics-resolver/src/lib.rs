//! Resolves Tomet's own file-referencing constructs -- today just
//! `@settings(file:...)`; `@import` is future work, see the TODO below.
//! This is Tomet's "preprocessor/linker" layer: the only place I/O
//! and file resolution are allowed, precisely because `tomet-parser`
//! itself must have none (see `docs/develop/architecture.md`'s
//! "Deterministic Static Parser Boundary").
//!
//! Deliberately does not depend on `tomet-semantics`: recognizing a
//! `@settings(file:...)` reference only needs a direct
//! `Sigil::At(Some("settings"))` match (see `settings_file_ref`), not a
//! full element-kind classification.
//!
//! ## Note: `docs/docs.settings.tmt` doesn't parse today
//! `docs/docs.settings.tmt` -- the real-world example this crate's tests
//! are modeled on -- currently fails to parse with `tomet-parser`
//! (`variant: "a"|"b"|"c"` is a `|`-union type syntax the parser's
//! `Value` grammar doesn't support; try `cargo run -p tomet -- check
//! docs/docs.settings.tmt` to reproduce). That's a pre-existing gap in
//! the schema/type-system design documented in
//! `docs/spec/types.tmt`, unrelated to this crate -- not
//! something introduced or fixed here. This crate's own test fixtures
//! (`tests/fixtures/valid_settings.tmt`) use a simplified value shape
//! that *does* parse today, so they only exercise the file-resolution
//! logic, not the not-yet-implemented type syntax.
//!
//! ## TODO: `@import`
//! Not designed yet -- no grammar or semantics for it exist anywhere in
//! `docs/`. Note: `docs/guide/cheatsheet.tmt` already contains
//! `@settings(import:../docs.settings.tmt)`, which uses a *file-relative*
//! path under an `import` key -- inconsistent with `file:`'s documented
//! *project-root-relative* semantics (`docs/guide/builtins/args.tmt`).
//! Don't treat that line as the spec for `@import`; it looks like
//! leftover/draft content, not a settled design.

mod connect;
mod error;
mod interp;
mod settings;

pub use connect::{RemoteConnection, resolve_connect_targets};
pub use error::ResolveError;
pub use interp::resolve_reference;
pub use settings::{
    config_import_ref, resolve_settings_file, resolve_settings_ref, settings_file_ref,
};
