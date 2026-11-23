//! One finding about a document, the same whatever found it.

use serde::{Deserialize, Serialize};
use tomet_position::LineIndex;
use tomet_span::Span;
use tomet_transform::{Unresolved, UnresolvedKind, UnresolvedLink};

/// How much a diagnostic matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// The document is wrong.
    Error,
    /// The document says something about itself, or leaves something unresolved, that a reader
    /// should see; it is otherwise fine. `@draft`, `@fixme` and an expression that has no value are
    /// warnings.
    Warning,
}

/// A place in the source, as a person counts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Position {
    /// The line, counted from 1.
    pub line: usize,
    /// The column, counted from 1, in characters.
    pub column: usize,
}

/// Something wrong with a document, or something it says about itself: what `tomet check` reports,
/// plus the links a page could not be given a target for.
///
/// What found it does not matter to whoever reads it: a document that does not read as the
/// vocabulary says, a link that goes nowhere and an expression that has no value all come as this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Diagnostic {
    /// What kind of finding it is, in lower case with hyphens: `unknown-element`, `dangling-anchor`,
    /// `draft`, `unresolved-link`, `unresolved-expression`. Stable: a tool can tell findings apart by it.
    pub code: String,
    /// How much it matters.
    pub severity: Severity,
    /// What it says, in words, for a person.
    pub message: String,
    /// Where it is, as a range of bytes of the source.
    pub span: Span,
    /// Where it starts, as a line and a column. `null` when the document was given as a tree and not as
    /// text, so there is no text to count in.
    pub start: Option<Position>,
    /// Where it ends, as a line and a column. `null` as `start` is.
    pub end: Option<Position>,
}

impl Diagnostic {
    fn new(
        code: &str,
        severity: Severity,
        message: String,
        span: Span,
        index: Option<&LineIndex<'_>>,
    ) -> Self {
        let position = |offset: usize| {
            index.map(|index| {
                let at = index.line_col(offset);
                Position {
                    line: at.line,
                    column: at.column,
                }
            })
        };
        Self {
            code: code.to_string(),
            severity,
            message,
            span,
            start: position(span.start),
            end: position(span.end),
        }
    }

    /// What `tomet check` found in a document.
    pub fn from_check(found: &tomet::validator::Diagnostic, index: Option<&LineIndex<'_>>) -> Self {
        let severity = match found.severity() {
            tomet::validator::Severity::Error => Severity::Error,
            tomet::validator::Severity::Warning => Severity::Warning,
        };
        let message = match index {
            Some(index) => found.message_in(index),
            None => found.to_string(),
        };
        Self::new(found.code(), severity, message, found.span(), index)
    }

    /// A `{$ ... }` that has no value, or an id that was dropped, when the document was prepared.
    pub fn from_unresolved(found: &Unresolved, index: Option<&LineIndex<'_>>) -> Self {
        let (code, message) = match found.kind {
            UnresolvedKind::Expression => (
                "unresolved-expression",
                format!("`{{$ {} }}` has no value: {}", found.source, found.reason),
            ),
            UnresolvedKind::DroppedId => (
                "dropped-id",
                format!("`{}` was dropped: {}", found.source, found.reason),
            ),
        };
        Self::new(code, Severity::Warning, message, found.span, index)
    }

    /// A link or an embed that has no target on the page.
    pub fn from_link(found: &UnresolvedLink, index: Option<&LineIndex<'_>>) -> Self {
        let what = if found.scheme == "embed" {
            format!("embed `{}`", found.target)
        } else {
            format!("link `{}:{}`", found.scheme, found.target)
        };
        Self::new(
            "unresolved-link",
            Severity::Error,
            format!("{what} goes nowhere: {}", found.reason),
            found.span,
            index,
        )
    }
}
