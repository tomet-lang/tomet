//! The parser's unit tests, by topic.

mod comments;
mod connects;
mod cst;
mod document_declarations;
mod elements;
mod fences;
mod groups;
mod holes;
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
    Block, Declaration, Element, Group, Inline, InterpExpr, InterpExprKind, Literal, Name,
    Paragraph, Placement, SoftBreak, Span, Value,
};

/// `parse_document`, and whenever it reads a document, a check that lowering the CST
/// of the same text gives the same tree: every string these tests parse is also a
/// case for lowering, edge cases and odd syntax included. A construct lowering does
/// not read yet is skipped, not failed.
fn parse_document(src: &str) -> crate::Result<tomet_ast::Document> {
    let parsed = crate::parse_document(src);
    if let Ok(document) = &parsed {
        match crate::lower_document(src, &crate::parse_cst(src)) {
            Ok(lowered) => assert_eq!(&lowered, document, "lowering differs for {src:?}"),
            Err(e) if e.message.starts_with(crate::UNSUPPORTED) => {}
            Err(e) => panic!("lowering failed for {src:?}: {e}"),
        }
    }
    parsed
}

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
/// `Element::list_item` stores them under `value` as `Group::Data`.
fn item_attrs(item: &tomet_ast::ListItem) -> Option<Value> {
    item.data.value.as_ref().map(|v| v.as_data())
}
