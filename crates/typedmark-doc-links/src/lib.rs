//! Broken-link checking for a vault of `.tm`/`.tmt` files: extracts every
//! `File`/`Embed`/`Tm`/`Ref` link (`Url`/`Id` are out of scope -- external
//! URLs need network requests, not a parsing/caching problem; `id:`
//! is a same-document id lookup with no cross-file aspect, better suited
//! to `typedmark-validator` as a separate lint), caches the extraction
//! per source file keyed by mtime (so re-checking a large vault doesn't
//! re-parse every unchanged file), and resolves every target against
//! what actually exists on disk -- `File`/`Embed`/`Tm` by path (see
//! `check`'s module doc for the exact resolution rules), and `Ref` by a
//! project-wide filename/stem search (`ref` values are names, not
//! paths).
//!
//! This crate is the "I/O and file resolution are allowed here" layer
//! for link-checking, analogous to `typedmark-doc-resolver`'s role for
//! `@settings(file:...)`/`${...}` resolution -- kept separate from that
//! crate since resolver's own scope is narrower and unrelated.

mod cache;
mod cache_path;
mod check;
mod collect;

pub use cache::{CacheOutcome, LinkCache, LinkCacheError};
pub use cache_path::default_cache_path;
pub use check::{BrokenLink, CheckReport, check_vault};
pub use collect::{DocumentLink, LinkKind, collect_links};
