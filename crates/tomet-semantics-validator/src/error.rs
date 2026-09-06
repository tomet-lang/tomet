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
    /// An element written with a bare name that is not one of Tomet's own.
    ///
    /// Bare names are reserved for the built-in vocabulary; a user-defined
    /// element must be namespaced. This is the diagnostic that replaces the
    /// old silent fall-back to `ElementKind::Custom`, which is where the
    /// official-vs-user-defined ambiguity actually lived.
    UnknownElement {
        name: String,
        /// Set when the name is namespaced and that namespace is not in
        /// scope -- a missing `@use` rather than a misspelled element.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unbound_namespace: Option<String>,
        span: Span,
    },
    /// A second copy of an element that may appear at most once.
    ///
    /// `singleton` was declared in `docs/spec/builtin-settings.tmt` for
    /// six elements and enforced nowhere, so a document with two `@meta`
    /// -- or two `@kind`, which is two answers to what the document is --
    /// passed every check this project had.
    DuplicateSingleton {
        name: String,
        first: Span,
        duplicate: Span,
    },
    /// An element that may only sit in the preamble, found after the
    /// document's body has started.
    ///
    /// The preamble is the run of such elements at the top of the file;
    /// the first block that is not one of them ends it. A `@kind` below
    /// that point is a directive nobody reading top-down would meet in
    /// time.
    OutsidePreamble { name: String, span: Span },
    /// An element written with the wrong sigil for its shape -- `@meta`
    /// instead of `#meta`, or `#em` instead of `@em`.
    ShapeMismatch {
        name: String,
        found: &'static str,
        expected: &'static str,
        span: Span,
    },
    /// An argument the element's vocabulary entry does not declare.
    ///
    /// Only reported for an element whose declaration has `@args` at all.
    /// No `@args` means "nothing said", not "takes none" -- saying the
    /// second is `@data`'s `open: false`, and there is no `@args`
    /// equivalent yet.
    UnknownArgument {
        element: String,
        argument: String,
        span: Span,
    },
    /// A `@param` marked `required: true` with no argument to fill it.
    MissingRequiredArgument {
        element: String,
        argument: String,
        span: Span,
    },
    /// A top-level `@settings`/`@config` key that has been retired.
    ///
    /// `elements:` described what a custom element takes -- its `args`,
    /// `required`, `positional`, `content`, `placement` and `singleton`.
    /// A `@vocabulary` document says all of that now, at the declaration
    /// rather than in a settings map beside it, so the settings copy was
    /// the second place one fact lived.
    ///
    /// It is reported rather than ignored because ignoring is how it got
    /// here: of the six keys only `positional` was ever read, by a
    /// `SettingsSchema` no caller ever built, so a document could declare
    /// `singleton: true` and be obeyed by nothing at all.
    RetiredSettingsKey { key: String, span: Span },
}

impl ValidationError {
    /// Returns the primary span where the validation violation occurred.
    pub fn span(&self) -> Span {
        match self {
            ValidationError::DuplicateId { duplicate, .. } => *duplicate,
            ValidationError::MissingRequiredMetaKey { span, .. } => *span,
            ValidationError::MissingRequiredSection { span, .. } => *span,
            ValidationError::UnknownElement { span, .. } => *span,
            ValidationError::ShapeMismatch { span, .. } => *span,
            ValidationError::DuplicateSingleton { duplicate, .. } => *duplicate,
            ValidationError::OutsidePreamble { span, .. } => *span,
            ValidationError::RetiredSettingsKey { span, .. } => *span,
            ValidationError::UnknownArgument { span, .. } => *span,
            ValidationError::MissingRequiredArgument { span, .. } => *span,
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
                title, id, kind, ..
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
            ValidationError::UnknownElement {
                name,
                unbound_namespace,
                ..
            } => {
                // One wording, in `tomet_semantics::UnknownName`. This arm
                // used to carry its own copy and the two drifted -- it was
                // still advising `@import(file:..., as:ns)` after that
                // element had split into `@use` and `@include`.
                write!(
                    f,
                    "{}",
                    tomet_semantics::UnknownName {
                        name: name.clone(),
                        unbound_namespace: unbound_namespace.clone(),
                    }
                )
            }
            ValidationError::DuplicateSingleton { name, first, .. } => {
                write!(
                    f,
                    "`{name}` may appear once in a document; the first is at {}:{}",
                    first.start.line, first.start.column
                )
            }
            ValidationError::OutsidePreamble { name, .. } => {
                write!(
                    f,
                    "`{name}` belongs in the preamble -- before the document's body \
                     starts, not after it"
                )
            }
            ValidationError::ShapeMismatch {
                name,
                found,
                expected,
                ..
            } => {
                // Not "use `#name`" any more. `#` stopped being the block
                // sigil when the shape axis was retracted, so the old
                // advice named an edit that cannot be made: `@` is the
                // only element sigil and shape comes from position.
                write!(
                    f,
                    "`{name}` is {expected}, but is written as {found}; \
                     shape comes from position -- {}",
                    if *expected == "a block element" {
                        "give it a line of its own, in block context"
                    } else {
                        "put it inside a paragraph, not alone on its line"
                    }
                )
            }
            ValidationError::UnknownArgument {
                element, argument, ..
            } => {
                write!(
                    f,
                    "`{element}` has no argument `{argument}`; its vocabulary \
                     declares the ones it takes with `@param`"
                )
            }
            ValidationError::MissingRequiredArgument {
                element, argument, ..
            } => {
                write!(f, "`{element}` requires the argument `{argument}`")
            }
            ValidationError::RetiredSettingsKey { key, .. } => {
                // Named per key rather than one generic sentence: `elements`
                // has somewhere to go and `types` does not, and telling a
                // reader to move something that has no destination is worse
                // than telling them it is gone.
                let advice = match key.as_str() {
                    "elements" => {
                        "declare elements in a `@vocabulary` document instead -- \
                         `@element` carries `display`, `region` and `singleton`, \
                         and `@param` carries the arguments (docs/spec/vocabulary.tmt)"
                    }
                    "types" => {
                        "it has no replacement: nothing read it, and per-element \
                         rendering style has no home in settings yet -- \
                         `format.callout.style` and `format.list.multiline.style` \
                         are the only two that are read"
                    }
                    _ => "remove it",
                };
                write!(f, "`{key}` in `@settings` is retired; {advice}")
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
