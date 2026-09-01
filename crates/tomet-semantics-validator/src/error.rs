use serde::{Deserialize, Serialize};
use std::fmt;
use tomet_ast::Span;
use tomet_cst::TextRange;

/// A `.tmt` schema/lint rule violation found by `validate_document`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationError {
    /// The same `id` value appears on more than one node. `first` is where
    /// it was first seen; `duplicate` is the later, offending occurrence.
    DuplicateId {
        id: String,
        first: Span,
        duplicate: Span,
    },
    /// A required `@meta` field defined in the blueprint is missing from the document.
    MissingRequiredMetaKey {
        key: String,
        kind: String,
        span: Span,
    },
    /// A required section/heading defined in the blueprint is missing from the document.
    MissingRequiredSection {
        title: String,
        id: Option<String>,
        kind: String,
        span: Span,
    },
}

impl ValidationError {
    /// Returns the primary span where the validation violation occurred.
    pub fn span(&self) -> Span {
        match self {
            ValidationError::DuplicateId { duplicate, .. } => *duplicate,
            ValidationError::MissingRequiredMetaKey { span, .. } => *span,
            ValidationError::MissingRequiredSection { span, .. } => *span,
        }
    }
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
            ValidationError::MissingRequiredMetaKey { key, kind, .. } => {
                write!(
                    f,
                    "document of kind `{kind}` is missing required @meta field `{key}`"
                )
            }
            ValidationError::MissingRequiredSection {
                title,
                id,
                kind,
                ..
            } => {
                if let Some(sec_id) = id {
                    write!(
                        f,
                        "document of kind `{kind}` is missing required section `{title}` (id: {sec_id})"
                    )
                } else {
                    write!(
                        f,
                        "document of kind `{kind}` is missing required section `{title}`"
                    )
                }
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// A validation error tied directly to precise byte ranges in the CST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CstValidationError {
    DuplicateId {
        id: String,
        first_range: TextRange,
        duplicate_range: TextRange,
    },
}

impl CstValidationError {
    pub fn range(&self) -> TextRange {
        match self {
            CstValidationError::DuplicateId {
                duplicate_range, ..
            } => *duplicate_range,
        }
    }
}

impl fmt::Display for CstValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CstValidationError::DuplicateId {
                id, first_range, ..
            } => {
                write!(
                    f,
                    "duplicate id `{id}` (first defined at byte offset {})",
                    u32::from(first_range.start())
                )
            }
        }
    }
}

impl std::error::Error for CstValidationError {}
