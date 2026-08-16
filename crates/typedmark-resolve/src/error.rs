use std::fmt;
use std::path::PathBuf;

/// Everything that can go wrong resolving a file-referencing construct
/// (today just `@settings(file:...)`).
#[derive(Debug)]
pub enum ResolveError {
    /// `path` couldn't be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// `path` was read, but failed to parse as TypedMark.
    Parse {
        path: PathBuf,
        source: typedmark_parser::Error,
    },
    /// `path` parsed fine, but has no top-level `@settings{ ... }` block.
    MissingSettingsBlock { path: PathBuf },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Io { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            ResolveError::Parse { path, source } => {
                write!(f, "failed to parse {}: {source}", path.display())
            }
            ResolveError::MissingSettingsBlock { path } => {
                write!(
                    f,
                    "{} has no top-level `@settings{{ ... }}` block",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ResolveError::Io { source, .. } => Some(source),
            ResolveError::Parse { source, .. } => Some(source),
            ResolveError::MissingSettingsBlock { .. } => None,
        }
    }
}
