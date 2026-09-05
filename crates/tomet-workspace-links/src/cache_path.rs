//! Where the link cache lives: the user's cache directory, never the
//! vault.
//!
//! The in-tree directories that survive -- `target/`, `result`,
//! `node_modules/`, `.venv/` -- are not there because they hold derived
//! data. They are there because something resolves that exact path: a
//! linker, a runtime's module lookup, an interpreter. This cache has no
//! such requirement, and pnpm shows the direction of travel -- it moved
//! `node_modules/`'s content into a global store and left behind only the
//! symlinks whose position means something.
//!
//! What decides it is that a vault is *synced*. Dropbox, iCloud, git. A
//! SQLite file inside a synced directory is a binary conflict waiting to
//! happen, and that is the difference between a vault and a project:
//! `.mypy_cache/` is never synced, a vault usually is. Obsidian's
//! `.obsidian/` is where this goes wrong in practice.
//!
//! Sharing one database across every vault needs no path-hashing scheme
//! to be invented: `files.path` is the primary key already and absolute
//! paths do not collide. The one real cost is rows left behind when a
//! vault is moved or deleted, which `LinkCache::prune_missing` pays off.

use std::path::PathBuf;

/// `~/.cache/tomet/links.sqlite3` on Unix, and the platform's equivalent
/// elsewhere.
///
/// Falls back to a directory under the system temp dir on a platform
/// that offers no cache directory at all -- still outside the vault,
/// which is the property that matters.
pub fn default_cache_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "tomet")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("tomet"))
        .join("links.sqlite3")
}
