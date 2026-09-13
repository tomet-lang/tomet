use serde::{Deserialize, Serialize};
use std::fmt;
use tomet_ast::Span;
use tomet_cst::TextRange;

/// How much a [`Diagnostic`] matters.
///
/// Derived from the variant rather than stored: severity is a property of
/// the rule, not of the occurrence, so keeping a field would be a second
/// place the same fact lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// The document is wrong. `tomet check` fails.
    Error,
    /// The document says something about itself that a reader should see,
    /// and is otherwise fine. `tomet check` reports it and still passes.
    Warning,
}

/// Something `validate_document` found and a reader should know about.
///
/// Named for what the callers already called it: `bindings/{js,java,
/// python}` each bind the result to `diagnostics`. It was
/// `ValidationError` while every variant really was an error; `Draft` and
/// `Fixme` are not, so the name moved rather than the meaning stretching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Diagnostic {
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
    /// `@draft[ ... ]` -- the document says it is unfinished here.
    ///
    /// A warning, not an error: an unfinished document is a normal state
    /// to be in and to commit, and the whole point of writing the marker
    /// is that a reader (and `tomet check`) can find it. Failing the run
    /// would make the honest thing the expensive one.
    ///
    /// This is why it is an element and not `//(TODO)`: `tomet_ast` has no
    /// comment node, so a comment marker cannot be counted here at all,
    /// and is dropped by anything that re-prints from the tree.
    Draft { note: String, span: Span },
    /// `@fixme[ ... ]` -- there is text here and it needs revisiting.
    Fixme { note: String, span: Span },
    /// A `:name(...)` connect whose name is not in the closed set
    /// `tomet_semantics::CONNECT_MEMBERS` declares (`rule` is the only
    /// member so far). Distinct from [`Diagnostic::UnknownElement`]: a
    /// connect is never fed through vocabulary resolution at all (see
    /// `tomet_tree::for_each_descendant`'s doc comment), so this is its
    /// own diagnostic rather than a special case of that one.
    UnknownConnect { name: String, span: Span },
    /// A descendant found inside an element carrying an active
    /// `:rule(allow:list(...))` whose classified name is not in that
    /// rule's `allow` list. `span` is the offending descendant's own
    /// span; `rule_span` is where the `:rule(...)` itself was written,
    /// so a reader can see both "found here" and "restricted here".
    DisallowedByRule {
        name: String,
        allowed: Vec<String>,
        rule_span: Span,
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

impl Diagnostic {
    /// Returns the primary span where the validation violation occurred.
    pub fn span(&self) -> Span {
        match self {
            Diagnostic::DuplicateId { duplicate, .. } => *duplicate,
            Diagnostic::MissingRequiredMetaKey { span, .. } => *span,
            Diagnostic::MissingRequiredSection { span, .. } => *span,
            Diagnostic::UnknownElement { span, .. } => *span,
            Diagnostic::UnknownConnect { span, .. } => *span,
            Diagnostic::DisallowedByRule { span, .. } => *span,
            Diagnostic::ShapeMismatch { span, .. } => *span,
            Diagnostic::DuplicateSingleton { duplicate, .. } => *duplicate,
            Diagnostic::OutsidePreamble { span, .. } => *span,
            Diagnostic::RetiredSettingsKey { span, .. } => *span,
            Diagnostic::UnknownArgument { span, .. } => *span,
            Diagnostic::MissingRequiredArgument { span, .. } => *span,
            Diagnostic::Draft { span, .. } => *span,
            Diagnostic::Fixme { span, .. } => *span,
        }
    }

    /// How much this matters. See [`Severity`].
    pub fn severity(&self) -> Severity {
        match self {
            Diagnostic::Draft { .. } | Diagnostic::Fixme { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Diagnostic::DuplicateId { id, first, .. } => {
                write!(
                    f,
                    "duplicate id `{id}` (first defined at {}:{})",
                    first.start.line, first.start.column
                )
            }
            Diagnostic::MissingRequiredMetaKey { key, kind, .. } => {
                write!(
                    f,
                    "document of kind `{kind}` is missing required @meta field `{key}`"
                )
            }
            Diagnostic::MissingRequiredSection {
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
            Diagnostic::UnknownElement {
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
            Diagnostic::UnknownConnect { name, .. } => {
                // One wording, in `tomet_semantics::UnknownConnectMember`,
                // for the same reason `UnknownElement` above delegates to
                // `UnknownName`: a second copy of the message here would
                // drift from the first change made to that one.
                write!(
                    f,
                    "{}",
                    tomet_semantics::UnknownConnectMember { name: name.clone() }
                )
            }
            Diagnostic::DisallowedByRule {
                name,
                allowed,
                rule_span,
                ..
            } => {
                write!(
                    f,
                    "`{name}` is not allowed here; the `:rule(allow:...)` at {}:{} permits only {}",
                    rule_span.start.line,
                    rule_span.start.column,
                    allowed.join(", ")
                )
            }
            Diagnostic::DuplicateSingleton { name, first, .. } => {
                write!(
                    f,
                    "`{name}` may appear once in a document; the first is at {}:{}",
                    first.start.line, first.start.column
                )
            }
            Diagnostic::OutsidePreamble { name, .. } => {
                write!(
                    f,
                    "`{name}` belongs in the preamble -- before the document's body \
                     starts, not after it"
                )
            }
            Diagnostic::ShapeMismatch {
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
            Diagnostic::UnknownArgument {
                element, argument, ..
            } => {
                write!(
                    f,
                    "`{element}` has no argument `{argument}`; its vocabulary \
                     declares the ones it takes with `@param`"
                )
            }
            Diagnostic::MissingRequiredArgument {
                element, argument, ..
            } => {
                write!(f, "`{element}` requires the argument `{argument}`")
            }
            Diagnostic::Draft { note, .. } => {
                if note.is_empty() {
                    write!(f, "unfinished here")
                } else {
                    write!(f, "unfinished here: {note}")
                }
            }
            Diagnostic::Fixme { note, .. } => {
                if note.is_empty() {
                    write!(f, "marked to fix")
                } else {
                    write!(f, "marked to fix: {note}")
                }
            }
            Diagnostic::RetiredSettingsKey { key, .. } => {
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

impl std::error::Error for Diagnostic {}

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
