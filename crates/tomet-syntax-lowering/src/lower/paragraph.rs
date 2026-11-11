//! Paragraphs and the running text in them.
//!
//! A paragraph's content range, how a run of tokens becomes `Text` and `SoftBreak`, and the
//! inline nodes in it: emphasis and its kin, code spans, autolinks and inline forms.

use super::error_node::lower_error;
use super::form::{LoweredSlot, lower_element, lower_slot, slot_of};
use super::{range_of, unsupported};
use crate::error::Result;
use crate::inline::{DELIMITERS, push_soft_break, push_text, trim_edges};
use tomet_ast::{Block, Data, Element, Inline, Name, Paragraph, Placement, RawText, Value};
use tomet_cst::{NodeOrToken, SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_span::Span;
use tomet_tree::{ElementExt, element_new};

/// The content range of a paragraph, computed from tree facts:
/// - starts at the first non-whitespace token or node
/// - ends before the trailing newline, or before the `]` when enclosed in a content group
fn paragraph_bounds(node: &SyntaxNode) -> Option<(usize, usize)> {
    let (_, node_end) = range_of(node);
    let first_content = node.children_with_tokens().find(|e| match e {
        SyntaxElement::Token(t) => t.kind() != K::WHITESPACE,
        SyntaxElement::Node(_) => true,
    })?;
    let start = usize::from(first_content.text_range().start());

    let mut end = if let Some(last) = node.last_token().filter(|t| t.kind() == K::NEWLINE) {
        usize::from(last.text_range().start())
    } else {
        node_end
    };

    if let Some(parent) = node.parent().filter(|p| p.kind() == K::CONTENT) {
        let bracket = parent
            .children_with_tokens()
            .find(|e| e.kind() == K::R_BRACKET);
        if let Some(bracket) = bracket {
            let close = usize::from(bracket.text_range().start());
            if close >= end {
                let between_is_ws = parent
                    .children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .filter(|t| {
                        let s = usize::from(t.text_range().start());
                        let e = usize::from(t.text_range().end());
                        s >= end && e <= close
                    })
                    .all(|t| t.kind() == K::WHITESPACE);
                if between_is_ws {
                    end = close;
                }
            }
        }
    }

    if start >= end {
        None
    } else {
        Some((start, end))
    }
}

/// A paragraph: running text, with the elements, spans and interpolations in it.
pub(super) fn lower_paragraph(node: &SyntaxNode) -> Result<Option<Block>> {
    let Some((start, end)) = paragraph_bounds(node) else {
        return Ok(None);
    };
    let mut content = lower_inlines(node, start, end)?;
    if content.is_empty() {
        return Ok(None);
    }
    // An interpolation that is the whole of its line, with nothing folded into it, is
    // a block of its own.
    if matches!(content.as_slice(), [Inline::Interp(_)]) {
        let Some(Inline::Interp(interp)) = content.pop() else {
            unreachable!()
        };
        return Ok(Some(Block::Interp(interp)));
    }
    Ok(Some(Block::Paragraph(Paragraph::new(
        content,
        Span::new(start, end),
    ))))
}

/// The running text in `parent` between byte offsets `from` and `to`:
/// tokens become `Text` and `SoftBreak`, comments are skipped, `\...` escapes
/// become their escaped punctuation run, and the ends are trimmed.
pub(super) fn lower_inlines(parent: &SyntaxNode, from: usize, to: usize) -> Result<Vec<Inline>> {
    let mut items = Vec::new();
    let mut pending_text = String::new();
    let mut pending_start = None;
    let mut pending_end = 0;

    let flush_text = |items: &mut Vec<Inline>,
                      pending_text: &mut String,
                      pending_start: &mut Option<usize>,
                      pending_end: &mut usize| {
        if let Some(start) = pending_start.take() {
            if !pending_text.is_empty() {
                push_text(
                    items,
                    std::mem::take(pending_text),
                    Span::new(start, *pending_end),
                );
            }
        }
    };

    let mut in_softbreak = false;
    let mut softbreak_start = 0;
    let mut softbreak_end = 0;

    let flush_softbreak = |items: &mut Vec<Inline>,
                           in_softbreak: &mut bool,
                           softbreak_start: usize,
                           softbreak_end: usize| {
        if *in_softbreak {
            *in_softbreak = false;
            push_soft_break(items, Span::new(softbreak_start, softbreak_end));
        }
    };

    let mut is_first_text = true;

    for child in parent.children_with_tokens() {
        let range = child.text_range();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        let beyond = match &child {
            SyntaxElement::Node(_) => start >= to,
            SyntaxElement::Token(t) if t.kind() == K::COMMENT => start >= to,
            SyntaxElement::Token(_) => end > to,
        };
        if start < from || beyond {
            continue;
        }
        match child {
            SyntaxElement::Token(t) if t.kind() == K::COMMENT => {
                flush_text(
                    &mut items,
                    &mut pending_text,
                    &mut pending_start,
                    &mut pending_end,
                );
                flush_softbreak(
                    &mut items,
                    &mut in_softbreak,
                    softbreak_start,
                    softbreak_end,
                );
                is_first_text = false;
            }
            SyntaxElement::Token(t) if t.kind() == K::NEWLINE => {
                flush_text(
                    &mut items,
                    &mut pending_text,
                    &mut pending_start,
                    &mut pending_end,
                );
                if !in_softbreak {
                    in_softbreak = true;
                    softbreak_start = start;
                }
                softbreak_end = end;
                is_first_text = false;
            }
            SyntaxElement::Token(t)
                if in_softbreak && matches!(t.kind(), K::WHITESPACE | K::MARGIN) =>
            {
                softbreak_end = end;
            }
            SyntaxElement::Token(t) => {
                flush_softbreak(
                    &mut items,
                    &mut in_softbreak,
                    softbreak_start,
                    softbreak_end,
                );
                if t.kind() == K::MARGIN {
                    continue;
                }
                let effective_start = if is_first_text && from < start {
                    from
                } else {
                    start
                };
                is_first_text = false;
                pending_start.get_or_insert(effective_start);
                pending_text.push_str(t.text());
                pending_end = end;
            }
            SyntaxElement::Node(n) if n.kind() == K::SLOT => {
                flush_softbreak(
                    &mut items,
                    &mut in_softbreak,
                    softbreak_start,
                    softbreak_end,
                );
                let effective_start = if is_first_text && from < start {
                    from
                } else {
                    start
                };
                is_first_text = false;
                pending_start.get_or_insert(effective_start);
                use std::fmt::Write;
                write!(pending_text, "{}", n.text()).unwrap();
                pending_end = end;
            }
            SyntaxElement::Node(n) if n.kind() == K::ESCAPE => {
                flush_softbreak(
                    &mut items,
                    &mut in_softbreak,
                    softbreak_start,
                    softbreak_end,
                );
                // A `\...` escape: the leading `\` is omitted, the escaped run is text.
                // The span starts after `\` (at start + 1) through end.
                let escaped_start = start + 1;
                let effective_start = if is_first_text && from < start {
                    from
                } else {
                    escaped_start
                };
                is_first_text = false;
                pending_start.get_or_insert(effective_start);
                for elem in n.children_with_tokens().skip(1) {
                    if let NodeOrToken::Token(t) = elem {
                        pending_text.push_str(t.text());
                    }
                }
                pending_end = end;
            }
            SyntaxElement::Node(n) if n.kind() == K::ERROR => {
                return Err(lower_error(&n));
            }
            SyntaxElement::Node(n) => {
                flush_softbreak(
                    &mut items,
                    &mut in_softbreak,
                    softbreak_start,
                    softbreak_end,
                );
                flush_text(
                    &mut items,
                    &mut pending_text,
                    &mut pending_start,
                    &mut pending_end,
                );
                is_first_text = false;
                items.push(lower_inline(&n)?);
            }
        }
    }
    if pending_start.is_some() && to > pending_end {
        pending_end = to;
    }
    flush_text(
        &mut items,
        &mut pending_text,
        &mut pending_start,
        &mut pending_end,
    );
    flush_softbreak(
        &mut items,
        &mut in_softbreak,
        softbreak_start,
        softbreak_end,
    );
    Ok(trim_edges(items))
}

/// One inline node of running text.
fn lower_inline(node: &SyntaxNode) -> Result<Inline> {
    match node.kind() {
        K::INLINE_FORM => lower_inline_form(node),
        K::EMPHASIS | K::STRONG | K::STRIKEOUT => lower_delimited(node).map(Inline::Element),
        K::CODE_SPAN => lower_code_span(node).map(Inline::Element),
        K::AUTOLINK => Ok(Inline::Element(lower_autolink(node))),
        K::ERROR => Err(lower_error(node)),
        other => Err(unsupported(format!("inline node {other:?}"))),
    }
}

/// `@name(...)[...]` in running text, or an interpolation.
fn lower_inline_form(node: &SyntaxNode) -> Result<Inline> {
    if let Some(slot) = slot_of(node) {
        return lower_slot(&slot).map(LoweredSlot::into_inline);
    }
    if crate::element::is_interpolation(node) {
        return crate::interp::lower_interpolation(node).map(Inline::Interp);
    }
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .and_then(|n| n.first_token())
        .map(|t| t.kind());
    match sigil {
        Some(K::AT) => lower_element(node).map(Inline::Element),
        _ => Err(unsupported("an inline form with this sigil")),
    }
}

/// `*em*`, `**strong**`, `~~strikeout~~` and the `_` spellings: an element whose
/// content is one paragraph, the text between the delimiters.
fn lower_delimited(node: &SyntaxNode) -> Result<Element> {
    let (start, end) = range_of(node);
    let text = node.text().to_string();
    let (delimiter, name) = DELIMITERS
        .iter()
        .find(|(d, _)| text.starts_with(d) && text.ends_with(d) && text.len() >= 2 * d.len())
        .ok_or_else(|| unsupported("a delimited span that does not read"))?;
    let inner = lower_inlines(node, start + delimiter.len(), end - delimiter.len())?;
    let span = Span::new(start, end);
    Ok(element_new(Name::bare(*name))
        .with_span(span)
        .with_content(vec![Block::Paragraph(Paragraph::new(inner, span))]))
}

/// A code span: a `raw` element whose content is the text between the single opening
/// and closing backticks.
fn lower_code_span(node: &SyntaxNode) -> Result<Element> {
    let (start, end) = range_of(node);
    if (end - start) < 2 {
        return Err(unsupported("a code span that does not read"));
    }
    let inner_span = Span::new(start + 1, end - 1);
    let mut inner = String::new();
    for t in node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .skip(1)
        .take_while(|t| usize::from(t.text_range().end()) <= (end - 1))
    {
        inner.push_str(t.text());
    }
    Ok(element_new(Name::bare("raw"))
        .with_placement(Placement::Inline)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(inner, inner_span))],
            inner_span,
        ))]))
}

/// A URL in running text: a `link` element whose `target` is the URL.
fn lower_autolink(node: &SyntaxNode) -> Element {
    let (start, end) = range_of(node);
    let mut element = element_new(Name::bare("link"))
        .with_placement(Placement::Inline)
        .with_span(Span::new(start, end));
    element.data = Data {
        args: Some(Value::Map(vec![(
            "target".to_string(),
            Value::String(node.text().to_string()),
        )])),
        value: None,
    };
    element
}
