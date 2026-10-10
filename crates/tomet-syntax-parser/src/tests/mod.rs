//! The parser's unit tests, by topic.

mod caret;
mod comments;
mod connects;
mod elements;
mod fences;
mod groups;
mod id;
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
    Block, Element, ElementValue, Inline, InterpExpr, InterpExprKind, Literal, Paragraph,
    Placement, Sigil, SoftBreak, Span, Value,
};

/// A `SoftBreak` for test expectations -- span never matters, `Span`'s
/// `PartialEq` always returns `true` (see its own doc comment).
fn sb() -> Inline {
    Inline::SoftBreak(SoftBreak {
        span: Span::dummy(),
    })
}

/// Wraps a flat inline sequence as `Element.content` (`Vec<Block>` now,
/// see `tmtroot/docs/spec/feature/content-shape.tmt`) -- every fixture
/// across these tests is still just one paragraph's worth. Replaces the
/// old `Some(vec![Inline::...])` shape as `wrap(vec![Inline::...])`.
fn wrap(inlines: Vec<Inline>) -> Option<Vec<Block>> {
    Some(vec![Block::Paragraph(tomet_ast::Paragraph::new(
        inlines,
        Span::dummy(),
    ))])
}

/// The single paragraph's own `Vec<Inline>` inside `Element.content`
/// (`Vec<Block>` now) -- for fixtures that still want to assert on a flat
/// inline run.
fn first_para(blocks: &[Block]) -> &[Inline] {
    match blocks {
        [Block::Paragraph(p)] => &p.content,
        _ => panic!("expected a single paragraph, got {blocks:?}"),
    }
}

/// A list item's trailing `{value}` attrs, if any -- mirrors how
/// `Element::list_item` stores them under `value` as `ElementValue::Data`.
fn item_attrs(item: &Element) -> Option<Value> {
    match &item.value {
        Some(v) => v.as_data(),
        _ => None,
    }
}
