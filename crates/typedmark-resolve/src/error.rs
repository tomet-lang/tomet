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
    /// A `${...}` `Identifier`/`Member` chain's root id has no matching
    /// `{id:...}`/`(id:...)` anywhere in the document.
    UnknownId { id: String },
    /// A `${...}` `Member` access's `member` key wasn't found -- either
    /// its base isn't a `Value::Map` at all, or it is one but doesn't
    /// have that key.
    NoSuchMember { member: String },
    /// A `Literal`/`Call` node isn't a reference -- only `Identifier`/
    /// `Member` chains are resolved here (a `Call` is
    /// `typedmark-compute`'s job).
    NotAReference,
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
            ResolveError::UnknownId { id } => {
                write!(f, "no element with id `{id}` found in the document")
            }
            ResolveError::NoSuchMember { member } => {
                write!(f, "no member `{member}` found")
            }
            ResolveError::NotAReference => {
                write!(
                    f,
                    "expression is not a reference (only identifiers and member access resolve here)"
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
            ResolveError::MissingSettingsBlock { .. }
            | ResolveError::UnknownId { .. }
            | ResolveError::NoSuchMember { .. }
            | ResolveError::NotAReference => None,
        }
    }
}
