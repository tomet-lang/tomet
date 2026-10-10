//! =[ tomet-load-config ]
//!
//! Finds the config that governs a path, by walking up from it, and
//! reads it. `tomet-config` owns the `Config` type and its pure parsing
//! (`load_config_from_str`); this crate is the filesystem half, split
//! out so `tomet-config` itself can stay layer 3 and disk-free.
//!
//! Shares the `tomet-load` name prefix deliberately: both crates are
//! "read something correctly off disk," one a document, one a config.
//! `tomet-load`'s own `Vault::discover` calls [`find_config_file`]
//! directly rather than duplicating the walk.

use std::path::{Path, PathBuf};

use tomet_config::{Config, ConfigError};

/// Searches `start` and its parent directories for `default.config.tmt` or `tomet.config.tmt`.
/// Returns `(Config, config_file_path, config_directory_path)`.
pub fn find_config_file(start: &Path) -> Option<(Config, PathBuf, PathBuf)> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let candidates = [
            current.join("default.config.tmt"),
            current.join("tomet.config.tmt"),
            current.join("default.config.tm"),
            current.join("tomet.config.tm"),
        ];
        for candidate in candidates {
            if candidate.exists()
                && let Ok(cfg) = load_config_from_file(&candidate)
            {
                let root = if current.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    current
                };
                return Some((cfg, candidate, root));
            }
        }
        if !current.pop() {
            break;
        }
    }
    None
}

pub fn load_config_from_file(path: &Path) -> Result<Config, ConfigError> {
    let src = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let doc = tomet_parser::parse_document(&src).map_err(|source| ConfigError::Parse {
        path: Some(path.to_path_buf()),
        source,
    })?;
    Ok(Config::from_doc(&doc))
}

/// The config that governs `path`, for a tool about to act on `src`.
///
/// The nearest `default.config.tmt`/`tomet.config.tmt` up the tree wins,
/// because that file's position is what says where the vault begins.
/// With no vault to find, the document's own `@config` is used, and
/// failing that the defaults.
pub fn config_for(path: Option<&Path>, src: &str) -> Config {
    if let Some(path) = path
        && let Some((cfg, _, _)) = find_config_file(path)
    {
        return cfg;
    }
    tomet_parser::parse_document(src)
        .map(|doc| Config::from_doc(&doc))
        .unwrap_or_default()
}
