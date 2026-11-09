//! Lists: items, the sublist under each, and the spans of both from the tree's own facts.

use super::form::lower_element;
use super::groups::lower_groups;
use super::paragraph::lower_inlines;
use super::{range_of, unsupported};
use crate::error::Result;
use crate::inline::{extend_merging, push_text};
use tomet_ast::{Block, List, Paragraph};
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_span::Span;
use tomet_tree::{list, list_item};

/// A list: its items, each with the sublist that follows it.
pub(super) fn lower_list(node: &SyntaxNode) -> Result<List> {
    let items = node
        .children()
        .filter(|n| n.kind() == K::LIST_ITEM)
        .map(|item| lower_list_item(&item))
        .collect::<Result<Vec<_>>>()?;
    let ordered = node
        .first_child()
        .and_then(|item| item.children_with_tokens().nth(1))
        .is_some_and(|e| e.kind() == K::DOT);
    let (first, last) = match (items.first(), items.last()) {
        (Some(first), Some(last)) => (first.span, last.span),
        _ => return Err(unsupported("an empty list")),
    };
    Ok(list(ordered, items, first.union(&last)))
}

/// The span of a list item, computed from tree facts:
/// - starts at line start when only blanks precede it on its line
/// - runs through its sublist when it has one, or through its trailing newline
fn list_item_span(node: &SyntaxNode, sublist_span_end: Option<usize>) -> Span {
    let start = list_item_span_start(node);
    let line_end = list_item_line_end(node);
    let end = match sublist_span_end {
        Some(sub_end) => sub_end,
        None => node
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == K::NEWLINE && usize::from(t.text_range().start()) == line_end)
            .map_or(line_end, |t| usize::from(t.text_range().end())),
    };
    Span::new(start, end)
}

fn list_item_span_start(node: &SyntaxNode) -> usize {
    let start = range_of(node).0;
    if let Some(prev) = node.prev_sibling_or_token() {
        if prev.kind() == K::WHITESPACE {
            let ws_start = usize::from(prev.text_range().start());
            let prev_prev = prev.prev_sibling_or_token();
            let only_blanks = match prev_prev {
                None => {
                    if let Some(parent) = node.parent() {
                        let prev_token = parent.first_token().and_then(|t| t.prev_token());
                        prev_token.is_none() || prev_token.map(|t| t.kind()) == Some(K::NEWLINE)
                    } else {
                        true
                    }
                }
                Some(p) => matches!(p.kind(), K::NEWLINE | K::LIST_ITEM),
            };
            if only_blanks {
                return ws_start;
            }
        }
    } else if let Some(parent) = node.parent().filter(|p| p.kind() == K::LIST) {
        if let Some(prev) = parent.prev_sibling_or_token() {
            if prev.kind() == K::WHITESPACE {
                let ws_start = usize::from(prev.text_range().start());
                let prev_token = prev.as_token().and_then(|t| t.prev_token());
                let only_blanks =
                    prev_token.is_none() || prev_token.map(|t| t.kind()) == Some(K::NEWLINE);
                if only_blanks {
                    return ws_start;
                }
            }
        }
    }
    start
}

fn list_item_line_end(node: &SyntaxNode) -> usize {
    let limit = sublist_start(node).unwrap_or_else(|| range_of(node).1);
    let from = node
        .children()
        .filter(|n| n.kind() != K::LIST)
        .map(|n| range_of(&n).1)
        .max()
        .unwrap_or_else(|| range_of(node).0)
        .min(limit);
    node.children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| {
            let s = usize::from(t.text_range().start());
            t.kind() == K::NEWLINE && s >= from && s <= limit
        })
        .map_or(limit, |t| usize::from(t.text_range().start()))
}

/// Appends trailing body text of a full-form or combine item to its content.
fn append_trailing_body(
    body: &SyntaxNode,
    content: &mut Vec<Block>,
    line_end: usize,
) -> Result<()> {
    let (body_start, body_end) = range_of(body);
    let first_content = body.children_with_tokens().find(|e| match e {
        SyntaxElement::Token(t) => t.kind() != K::WHITESPACE,
        SyntaxElement::Node(_) => true,
    });
    let Some(first_content) = first_content else {
        return Ok(());
    };
    let content_start = usize::from(first_content.text_range().start());
    let rest = lower_inlines(body, content_start, body_end)?;
    if rest.is_empty() {
        return Ok(());
    }
    if content_start > body_start
        && let Some(Block::Paragraph(p)) = content.last_mut()
    {
        push_text(
            &mut p.content,
            " ".to_string(),
            Span::new(body_start, content_start),
        );
    }
    match content.last_mut() {
        Some(Block::Paragraph(p)) => extend_merging(&mut p.content, rest),
        _ => content.push(Block::Paragraph(Paragraph::new(
            rest,
            Span::new(content_start, line_end),
        ))),
    }
    Ok(())
}

/// One item. Its shapes: `- text`, `- (marker) text`, the combine notation
/// `-@name(...)[...] text`, and the full form, where a group opens on the marker
/// (`- [ content ] text`). The groups before the text are the marker's and the
/// full form's; those after it are the ones a sugar body ends in.
fn lower_list_item(node: &SyntaxNode) -> Result<tomet_ast::ListItem> {
    let sublist = match node.children().find(|n| n.kind() == K::LIST) {
        Some(sub) => Some(lower_list(&sub)?),
        None => None,
    };
    let span = list_item_span(node, sublist.as_ref().map(|s| s.span.end));
    let line_end = list_item_line_end(node);

    let is_group = |n: &SyntaxNode| {
        matches!(
            n.kind(),
            K::ARGS | K::VALUE_DATA | K::CONTENT | K::ID_GROUP | K::CONNECT
        )
    };
    let combine = node.children().find(|n| n.kind() == K::INLINE_FORM);
    let body = node.children().find(|n| n.kind() == K::BODY);

    let mut item = if let Some(element) = combine {
        if node.children().any(|n| is_group(&n)) {
            return Err(unsupported("a combine item with other groups"));
        }
        let el = lower_element(&element)?;
        let mut content = el.content.unwrap_or_default();
        if let Some(body) = &body {
            append_trailing_body(body, &mut content, line_end)?;
        }
        tomet_ast::ListItem {
            name: Some(el.name),
            data: el.data,
            content,
            id: el.id,
            connects: el.connects,
            sublist: None,
            span,
        }
    } else {
        let read = lower_groups(node)?;
        let full_form = node
            .children()
            .take_while(|n| n.kind() != K::BODY)
            .any(|n| matches!(n.kind(), K::VALUE_DATA | K::CONTENT | K::ID_GROUP));
        let attrs = read.data.value.map(|v| v.as_data());
        let content = if full_form {
            let mut blocks = read.content.unwrap_or_default();
            if let Some(body) = &body {
                append_trailing_body(body, &mut blocks, line_end)?;
            }
            blocks
        } else if let Some(body) = &body {
            let (from, to) = range_of(body);
            let inlines = lower_inlines(body, from, to)?;
            if inlines.is_empty() {
                Vec::new()
            } else {
                vec![Block::Paragraph(Paragraph::new(
                    inlines,
                    Span::new(from, line_end),
                ))]
            }
        } else {
            Vec::new()
        };
        list_item(
            None,
            content,
            read.data.args,
            attrs,
            read.id,
            read.connects,
            None,
            span,
        )
    };
    item.sublist = sublist;
    Ok(item)
}

fn sublist_start(node: &SyntaxNode) -> Option<usize> {
    node.children()
        .find(|n| n.kind() == K::LIST)
        .map(|sub| range_of(&sub).0)
}
