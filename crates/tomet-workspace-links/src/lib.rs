//! =[ tomet-links ]
//!
//! Vault-wide link target resolution, and -- behind the `cache` feature --
//! broken-link detection with a SQLite-backed mtime cache.
//!
//! =[ Features ]
//!
//! - (`cache` (on by default))
//!   -| The SQLite incremental cache ([`LinkCache`], [`default_cache_path`]) and the
//!    | vault-wide check built on it ([`check_vault`]). Switch it off with
//!    | `default-features = false` to use [`VaultLinkIndex`] and [`collect_links`] with
//!    | no SQLite in the build -- a host that links its own `libsqlite3` cannot also
//!    | link this crate's (Cargo allows one copy of a native library per build).
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Vault-Wide Link Integrity & Resolution Layer)
//!   -| `tomet-links` scans workspaces and checks `File`, `Embed`, `Tm`, and `Ref` links
//!    | for validity against real files on disk. External URLs (`Url`) and intra-document
//!    | identifiers (`Id`) are deliberately excluded (network checkers or `tomet-validator`).
//! - (Link Target Resolution Rules)
//!   -| `./` or `../`: Relative to referencing file's directory.
//!    | `/`: OS-absolute path.
//!    | Bare string: Relative to project root.
//!    | `Ref`: Case/extension-insensitive matching against file names and stems (wikilink convention).
//!    | `Tm`: Project-root-relative path with `#fragment` stripped.
//! - (SQLite Incremental Cache ([`LinkCache`]))
//!   -| A cache database in `~/.cache/tomet/links.sqlite3` avoids re-parsing unchanged
//!    | files across runs based on filesystem modification timestamps (`mtime`).

#[cfg(feature = "cache")]
mod cache;
#[cfg(feature = "cache")]
mod cache_path;
#[cfg(feature = "cache")]
mod check;
mod collect;
mod resolver;

#[cfg(feature = "cache")]
pub use cache::{CacheOutcome, LinkCache, LinkCacheError};
#[cfg(feature = "cache")]
pub use cache_path::default_cache_path;
#[cfg(feature = "cache")]
pub use check::{BrokenLink, CheckReport, check_vault};
pub use collect::{DocumentLink, LinkKind, collect_links};
pub use resolver::VaultLinkIndex;
