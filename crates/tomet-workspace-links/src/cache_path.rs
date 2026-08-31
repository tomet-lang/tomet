//! Default location for a vault's link cache database. Vault-local
//! rather than a global `directories`-based cache dir: no path-hashing
//! scheme needs inventing, it's trivially deletable to force a rebuild,
//! and it doesn't produce orphaned entries when a vault moves or is
//! deleted. Costs one `.gitignore` line per vault, the same as `.git/`/
//! `target/`/`node_modules/` already do elsewhere.

use std::path::{Path, PathBuf};

/// `<config_root>/.tomet/links-cache.sqlite3`.
pub fn default_cache_path(config_root: &Path) -> PathBuf {
    config_root.join(".tomet").join("links-cache.sqlite3")
}
