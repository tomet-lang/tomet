//! Configuration discovery, file loading, and error reporting.

use std::path::{Path, PathBuf};

use crate::Config;

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: Option<PathBuf>,
        source: tomet_parser::Error,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "failed to read config file {}: {source}", path.display())
            }
            ConfigError::Parse {
                path: Some(path),
                source,
            } => {
                write!(
                    f,
                    "failed to parse config file {}: {source}",
                    path.display()
                )
            }
            ConfigError::Parse { path: None, source } => {
                write!(f, "failed to parse config source: {source}")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::Parse { source, .. } => Some(source),
        }
    }
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

pub fn load_config_from_str(src: &str) -> Result<Config, ConfigError> {
    let doc = tomet_parser::parse_document(src)
        .map_err(|source| ConfigError::Parse { path: None, source })?;
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
