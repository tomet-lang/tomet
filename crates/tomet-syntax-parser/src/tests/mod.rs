//! The parser's unit tests, by topic.

mod caret;
mod comments;
mod connects;
mod elements;
mod fences;
mod groups;
mod inline;
mod interpolation;
mod lists;
mod paragraphs;
mod sections;
mod thematic;
mod urls;
mod values;

use super::*;
use tomet_ast::{
    Block, Element, ElementValue, Inline, InterpExpr, InterpExprKind, Literal, Sigil, SoftBreak,
    Span, Value,
};
use tomet_semantics::{list_items, list_ordered};

/// A `SoftBreak` for test expectations -- span never matters, `Span`'s
/// `PartialEq` always returns `true` (see its own doc comment).
fn sb() -> Inline {
    Inline::SoftBreak(SoftBreak {
        span: Span::dummy(),
    })
}
