use std::fmt;

/// Everything that can go wrong resolving a `${...}` `Identifier`/`Member`
/// chain against a document's own `#(id)`-tagged nodes.
#[derive(Debug)]
pub enum ResolveError {
    /// A `${...}` `Identifier`/`Member` chain's root id has no matching
    /// `{id:...}`/`(id:...)` anywhere in the document.
    UnknownId { id: String },
    /// A `${...}` `Member` access's `member` key wasn't found -- either
    /// its base isn't a `Value::Map` at all, or it is one but doesn't
    /// have that key.
    NoSuchMember { member: String },
    /// A `Literal`/`Call` node isn't a reference -- only `Identifier`/
    /// `Member` chains are resolved here (a `Call` is
    /// `tomet-compute`'s job).
    NotAReference,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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

impl std::error::Error for ResolveError {}
