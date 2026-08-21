use std::fmt;
use typedmark_ast::Span;

/// A `.tm` schema/lint rule violation found by `validate_document`.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// The same `id` value appears on more than one node. `first` is where
    /// it was first seen; `duplicate` is the later, offending occurrence.
    DuplicateId {
        id: String,
        first: Span,
        duplicate: Span,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::DuplicateId { id, first, .. } => {
                write!(
                    f,
                    "duplicate id `{id}` (first defined at {}:{})",
                    first.start.line, first.start.column
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}
