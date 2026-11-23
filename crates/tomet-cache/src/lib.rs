//! =[ tomet-cache ]
//!
//! Pure cache protocol for Tomet, following `tmtroot/spec/features/cache.tmt`.
//!
//! This crate performs no filesystem I/O (`std::fs` is forbidden by `.writ.tmt`
//! `@rule(file-io-doors)`). Disk reading lives in `tomet-load` (Layer 7), while
//! external cache producers live outside the `tomet` repository.
//!
//! =[ Responsibilities ]
//!
//! - (Hashing & Layout)
//!   | [`key_hash`], [`vault_id`], and [`entry_rel_path`] compute SHA-256 hex
//!   | digests and `<kind>/<sha256>.cache.ctmt` relative paths.
//! - (Cache Entry Format)
//!   | [`CacheEntry`], [`parse_cache_entry`], and [`format_cache_entry`] read
//!   | and write `.cache.ctmt` documents (`::kind(cache)`, `key`, `value`,
//!   | optional `meta`).
//! - (AST Extraction)
//!   | [`collect_cache_requests`] walks a parsed [`tomet_ast::File`] to extract
//!   | all `$cache.<kind>(key, local: ..., inputs: ...)` calls as
//!   | [`LocatedCacheRequest`]s.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

mod extract;
mod format;

pub use extract::{
    LocatedCacheRequest, collect_cache_requests, collect_cache_requests_from_data_file,
};
pub use format::{CacheEntry, CacheParseError, format_cache_entry, parse_cache_entry};
pub use tomet_expr::CacheRequest;

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Returns the lowercase 64-character hex SHA-256 digest of `key`'s UTF-8 bytes.
pub fn key_hash(key: &str) -> String {
    sha256_hex(key.as_bytes())
}

/// Returns the lowercase 64-character hex SHA-256 vault identifier computed from
/// the UTF-8 bytes of `canonical_root.to_string_lossy()`.
pub fn vault_id(canonical_root: &Path) -> String {
    sha256_hex(canonical_root.to_string_lossy().as_bytes())
}

/// Returns the relative cache entry path `<kind>/<key_hash(key)>.cache.ctmt`.
pub fn entry_rel_path(kind: &str, key: &str) -> PathBuf {
    PathBuf::from(kind).join(format!("{}.cache.ctmt", key_hash(key)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_hash_matches_known_sha256_vectors() {
        // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(
            key_hash(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        assert_eq!(
            key_hash("hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(key_hash("日本語キー").len(), 64);
        assert!(key_hash("hello").bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    }

    #[test]
    fn vault_id_hashes_root_path_bytes() {
        let root = Path::new("/tmp/my-vault");
        assert_eq!(vault_id(root), key_hash("/tmp/my-vault"));
    }

    #[test]
    fn entry_rel_path_uses_kind_and_sha256_suffix() {
        let expected_name = format!("{}.cache.ctmt", key_hash("whoami"));
        assert_eq!(
            entry_rel_path("command", "whoami"),
            PathBuf::from("command").join(expected_name)
        );
    }
}
