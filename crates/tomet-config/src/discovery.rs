//! Config parsing and error reporting. Finding and reading a config file
//! off disk is `tomet-load-config`'s job, one layer up -- this stays
//! pure so `tomet-config` itself can stay layer 3 and disk-free.
//! [`ConfigError::Io`] exists for that crate's `load_config_from_file`
//! to construct; nothing in this crate ever produces it.

use std::path::PathBuf;

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

pub fn load_config_from_str(src: &str) -> Result<Config, ConfigError> {
    let doc = tomet_parser::parse_document(src)
        .map_err(|source| ConfigError::Parse { path: None, source })?;
    Ok(Config::from_doc(&doc))
}
