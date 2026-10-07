//! =[ tomet-links ]
//!
//! Vault-wide broken-link detection, target resolution, and SQLite-backed mtime caching.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - Vault-Wide Link Integrity & Resolution Layer:
//!   `tomet-links` scans workspaces and checks `File`, `Embed`, `Tm`, and `Ref` links
//!   for validity against real files on disk. External URLs (`Url`) and intra-document
//!   identifiers (`Id`) are deliberately excluded (network checkers or `tomet-validator`).
//!
//! - Link Target Resolution Rules:
//!   -| `./` or `../`: Relative to referencing file's directory.
//!    | `/`: OS-absolute path.
//!    | Bare string: Relative to project root.
//!    | `Ref`: Case/extension-insensitive matching against file names and stems (wikilink convention).
//!    | `Tm`: Project-root-relative path with `#fragment` stripped.
//!
//! - SQLite Incremental Cache ([`LinkCache`]):
//!   A cache database in `~/.cache/tomet/links.sqlite3` avoids re-parsing unchanged
//!   files across runs based on filesystem modification timestamps (`mtime`).

mod cache;
mod cache_path;
mod check;
mod collect;
mod resolver;

pub use cache::{CacheOutcome, LinkCache, LinkCacheError};
pub use cache_path::default_cache_path;
pub use check::{BrokenLink, CheckReport, check_vault};
pub use collect::{DocumentLink, LinkKind, collect_links};
pub use resolver::VaultLinkIndex;
