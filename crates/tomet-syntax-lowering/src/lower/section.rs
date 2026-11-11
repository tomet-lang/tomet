//! Sections: the heading (level, title, groups) and the blocks under it.

use super::groups::lower_groups;
use super::paragraph::lower_inlines;
use super::{lower_block, range_of, unsupported};
use crate::error::Result;
use crate::value::err;
use tomet_ast::{Block, Inline, Placement, Section};
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_span::Span;

/// A section: its heading (level, title, groups) and the blocks under it.
///
/// Only the headings with no title, with a title of plain text, or with a
/// `[title]` of one plain paragraph are lowered; a title with an element in it
/// is not yet.
pub(super) fn lower_section(node: &SyntaxNode) -> Result<Block> {
    let heading = node
        .children()
        .find(|n| n.kind() == K::SECTION_HEADING)
        .ok_or_else(|| unsupported("a section without a heading"))?;
    let (start, end) = range_of(&heading);
    let level = heading
        .children_with_tokens()
        .take_while(|e| e.kind() == K::EQUAL)
        .count();
    let read = lower_groups(&heading)?;

    let title = if let Some(content) = read.content.clone() {
        let at = heading
            .children()
            .find(|n| n.kind() == K::CONTENT)
            .map_or(end, |n| range_of(&n).1);
        title_from_content(at, content)?
    } else {
        sugar_title(&heading, level)?
    };
    let mut section = Section::new(level, title, Span::new(start, end));
    section.data = read.data;
    section.id = read.id;
    section.connects = read.connects;
    for child in node.children().filter(|n| n.kind() != K::SECTION_HEADING) {
        section.blocks.extend(lower_block(&child)?);
    }
    Ok(Block::Section(section))
}

/// The inline content of a `[title]`. Empty content is an empty title; one plain
/// paragraph is that paragraph's own inline content; a single element, hole or
/// interpolation (`=[ $(x) ]`) is demoted to inline and is the title's only node.
/// Anything else has no place in `Section.title`, which holds inlines.
fn title_from_content(at: usize, mut blocks: Vec<Block>) -> Result<Vec<Inline>> {
    let only = |what: &str| {
        err(
            at,
            format!("a heading's title can only hold plain text, not {what}"),
        )
    };
    match blocks.len() {
        0 => Ok(Vec::new()),
        1 => match blocks.pop() {
            Some(Block::Paragraph(p)) => Ok(p.content),
            Some(Block::Element(mut el)) => {
                el.placement = Placement::Inline;
                Ok(vec![Inline::Element(el)])
            }
            Some(Block::Interp(interp)) => Ok(vec![Inline::Interp(interp)]),
            Some(Block::Hole(mut hole)) => {
                hole.placement = Placement::Inline;
                Ok(vec![Inline::Hole(hole)])
            }
            Some(Block::TypeSeat(seat)) => Ok(vec![Inline::TypeSeat(seat)]),
            Some(Block::Section(_)) => Err(only("a nested section")),
            Some(Block::List(_)) => Err(only("a list")),
            None => unreachable!(),
        },
        _ => Err(err(
            at,
            "a heading's title can only hold one paragraph of plain text, not multiple blocks",
        )),
    }
}

/// `== text`: the heading's text, up to the groups it ends in.
fn sugar_title(heading: &SyntaxNode, _level: usize) -> Result<Vec<Inline>> {
    let body = heading.children().find(|n| n.kind() == K::BODY);
    let Some(body) = body else {
        return Ok(Vec::new());
    };
    let (from, to) = range_of(&body);
    let mut title = lower_inlines(&body, from, to)?;
    let ends_with_unescaped_eq = body
        .children_with_tokens()
        .filter(|e| !e.kind().is_trivia())
        .last()
        .is_some_and(|e| matches!(e, SyntaxElement::Token(t) if t.kind() == K::EQUAL));
    if ends_with_unescaped_eq {
        trim_trailing_equals(&mut title);
    }
    Ok(title)
}

fn trim_trailing_equals(inlines: &mut Vec<Inline>) {
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        let trimmed = t.value.trim_end();
        if trimmed.ends_with('=') {
            let before_eq = trimmed.trim_end_matches('=');
            if before_eq.is_empty() || before_eq.ends_with(char::is_whitespace) {
                t.value = before_eq.trim_end().to_string();
                if t.value.is_empty() {
                    inlines.pop();
                }
            }
        }
    }
}
