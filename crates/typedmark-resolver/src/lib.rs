//! Resolves TypedMark's own file-referencing constructs -- today just
//! `@settings(file:...)`; `@import` is future work, see the TODO below.
//! This is TypedMark's "preprocessor/linker" layer: the only place I/O
//! and file resolution are allowed, precisely because `typedmark-parser`
//! itself must have none (see `docs/develop/architecture.md`'s
//! "Deterministic Static Parser Boundary").
//!
//! Deliberately does not depend on `typedmark-semantics`: recognizing a
//! `@settings(file:...)` reference only needs a direct
//! `Sigil::At(Some("settings"))` match (see `settings_file_ref`), not a
//! full element-kind classification.
//!
//! ## Note: `docs/docs.settings.tm` doesn't parse today
//! `docs/docs.settings.tm` -- the real-world example this crate's tests
//! are modeled on -- currently fails to parse with `typedmark-parser`
//! (`variant: "a"|"b"|"c"` is a `|`-union type syntax the parser's
//! `Value` grammar doesn't support; try `cargo run -p typedmark -- check
//! docs/docs.settings.tm` to reproduce). That's a pre-existing gap in
//! the schema/type-system design documented in
//! `docs/ja/specifications/types.tm`, unrelated to this crate -- not
//! something introduced or fixed here. This crate's own test fixtures
//! (`tests/fixtures/valid_settings.tm`) use a simplified value shape
//! that *does* parse today, so they only exercise the file-resolution
//! logic, not the not-yet-implemented type syntax.
//!
//! ## TODO: `@import`
//! Not designed yet -- no grammar or semantics for it exist anywhere in
//! `docs/`. Note: `docs/ja/cheatsheet.tm` already contains
//! `@settings(import:../docs.settings.tm)`, which uses a *file-relative*
//! path under an `import` key -- inconsistent with `file:`'s documented
//! *project-root-relative* semantics (`docs/ja/specifications/builtin-args.tm`).
//! Don't treat that line as the spec for `@import`; it looks like
//! leftover/draft content, not a settled design.

mod connect;
mod error;
mod interp;
mod settings;

pub use connect::{RemoteConnection, resolve_connect_targets};
pub use error::ResolveError;
pub use interp::resolve_reference;
pub use settings::{resolve_settings_file, resolve_settings_ref, settings_file_ref};
