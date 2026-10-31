//! Lowering: the CST read as an AST.
//!
//! The CST parser is the one reader of the grammar's shape (where a block, an
//! element, a group starts and ends); the AST is the owned, typed tree the rest
//! of the system works on. Lowering is the fold from the first to the second, so
//! that a construct is written once, in the CST parser, and the AST parser stops
//! being a second implementation of it.
//!
//! It is built up construct by construct. [`lower_document`] returns an
//! [`unsupported`] error for a node it does not lower yet, and the differential
//! test in `tests/src/lowering.rs` compares what it does lower with
//! `parse_document`, over every fixture and every string the parser's own tests
//! use. Nothing in the AST parser is deleted until that test passes for all of
//! them.
//!
//! What stays shared with the AST parser is not grammar but text handling: how a
//! run of source text becomes `Text` and `SoftBreak` items
//! ([`crate::inline::split_softbreaks`]), and the grammars of the leaves that are
//! not shape (`${...}` expressions, a group's values).

use crate::error::{Error, Result};
use crate::inline::{split_softbreaks, trim_edges};
use tomet_ast::{Block, Document, Inline, Name, Paragraph, Placement, RawText, Span, Value};
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_tree::{ElementExt, element_new};

/// The message of the error [`lower_document`] returns for a construct it does
/// not lower yet. The differential test counts these instead of failing on them.
pub const UNSUPPORTED: &str = "not lowered yet";

fn unsupported(what: impl std::fmt::Display) -> Error {
    Error {
        message: format!("{UNSUPPORTED}: {what}"),
        line: 0,
        column: 0,
        offset: 0,
    }
}

/// Reads the CST of `src` as an AST [`Document`].
#[doc(hidden)]
pub fn lower_document(src: &str, root: &SyntaxNode) -> Result<Document> {
    let blocks = lower_blocks(src, root)?;
    Ok(Document::new(blocks, Span::new(0, src.len())))
}

/// Each top-level block of the document lowered on its own, with the offset
/// the CST node starts at (its first character that is not white space), so a
/// caller can match it with the AST's block there and see what is left to do.
#[doc(hidden)]
pub fn lower_top_level_blocks(src: &str, root: &SyntaxNode) -> Vec<(usize, Result<Option<Block>>)> {
    root.children()
        .map(|n| {
            let text = n.text().to_string();
            let start = usize::from(n.text_range().start()) + (text.len() - text.trim_start().len());
            (start, lower_block(src, &n))
        })
        .collect()
}

/// The blocks that are children of `node`: the document, and (later) a section
/// or a `[content]` group.
fn lower_blocks(src: &str, node: &SyntaxNode) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();
    for child in node.children_with_tokens() {
        match child {
            // Trivia, and the `;` that only breaks a block, leave nothing behind.
            SyntaxElement::Token(t) if t.kind().is_trivia() || t.kind() == K::SEMI => {}
            SyntaxElement::Token(t) => return Err(unsupported(format!("token {:?}", t.kind()))),
            SyntaxElement::Node(n) => blocks.extend(lower_block(src, &n)?),
        }
    }
    Ok(blocks)
}

/// One block node. `None` is a block that leaves nothing in the AST (an empty paragraph).
fn lower_block(src: &str, node: &SyntaxNode) -> Result<Option<Block>> {
    match node.kind() {
        K::PARAGRAPH => lower_paragraph(src, node),
        K::THEMATIC_BREAK => lower_thematic_break(src, node).map(Some),
        K::CODE_BLOCK => lower_code_block(src, node).map(Some),
        other => Err(unsupported(format!("node {other:?}"))),
    }
}

/// A paragraph of plain text: no element, no interpolation, no comment in it.
fn lower_paragraph(src: &str, node: &SyntaxNode) -> Result<Option<Block>> {
    if node.children().next().is_some() {
        return Err(unsupported("a paragraph with an element in it"));
    }
    if node
        .children_with_tokens()
        .any(|e| e.kind() == K::COMMENT)
    {
        return Err(unsupported("a comment in a paragraph"));
    }
    // From the first character that is not white space to the end of the last
    // line: the node also holds the newline that ends the paragraph.
    let range = node.text_range();
    let (node_start, node_end) = (usize::from(range.start()), usize::from(range.end()));
    let text = &src[node_start..node_end];
    let lead = text.len() - text.trim_start().len();
    let start = node_start + lead;
    let end = node_start + text.trim_end_matches(['\n', '\r']).len();
    if start >= end {
        return Ok(None);
    }
    let mut items = Vec::new();
    // A `\;` at the start of a line is a `;` that is text: the backslash is not
    // part of it.
    let mut from = start;
    for at in escaped_semicolons(src, start, end) {
        split_softbreaks(&mut items, &src[from..at], from, &[]);
        from = at + 1;
    }
    split_softbreaks(&mut items, &src[from..end], from, &[]);
    let content = trim_edges(items);
    if content.is_empty() {
        return Ok(None);
    }
    Ok(Some(Block::Paragraph(Paragraph::new(
        content,
        Span::new(start, end),
    ))))
}

/// The offsets of the `\` of every `\;` in `src[start..end]` that opens a line.
fn escaped_semicolons(src: &str, start: usize, end: usize) -> Vec<usize> {
    let text = &src[start..end];
    text.match_indices("\\;")
        .map(|(i, _)| start + i)
        .filter(|&at| {
            let line = &src[..at];
            let from = line.rfind('\n').map_or(0, |n| n + 1);
            line[from..].chars().all(char::is_whitespace)
        })
        .collect()
}

/// The source text of `node` and where it starts.
fn text_of<'a>(src: &'a str, node: &SyntaxNode) -> (usize, &'a str) {
    let range = node.text_range();
    let (start, end) = (usize::from(range.start()), usize::from(range.end()));
    (start, &src[start..end])
}

/// `---`: a run of dashes on a line of its own. A title (`---[ T ]---`) is not
/// lowered yet.
fn lower_thematic_break(src: &str, node: &SyntaxNode) -> Result<Block> {
    if node.children().next().is_some() {
        return Err(unsupported("a titled thematic break"));
    }
    if node.children_with_tokens().any(|e| e.kind() == K::COMMENT) {
        return Err(unsupported("a comment after a thematic break"));
    }
    let (start, text) = text_of(src, node);
    let end = start + text.trim_end_matches(['\n', '\r']).len();
    Ok(Block::Element(
        element_new(Name::bare("hr"))
            .with_placement(Placement::Block)
            .with_span(Span::new(start, end)),
    ))
}

/// A fenced code block: a `raw` element whose content is the body, verbatim,
/// and whose `lang` is the first word of the line the fence opens on.
fn lower_code_block(src: &str, node: &SyntaxNode) -> Result<Block> {
    if node.descendants_with_tokens().any(|e| e.kind() == K::PIPE) {
        return Err(unsupported("a fenced code block in a `|` run"));
    }
    let (start, text) = text_of(src, node);
    let fence_len = text.chars().take_while(|&c| c == '`').count();
    let (opening, rest) = match text.find('\n') {
        Some(n) => (&text[fence_len..n], &text[n + 1..]),
        None => (&text[fence_len..], ""),
    };
    let lang = opening.split_whitespace().next().unwrap_or("").to_string();
    let body_start = start + (text.len() - rest.len());

    // The body ends where a closing line starts: a run of at least as many
    // backticks, and nothing else but white space.
    let last_line_at = rest.rfind('\n').map_or(0, |n| n + 1);
    let last_line = &rest[last_line_at..];
    let closes = {
        let ticks = last_line.chars().take_while(|&c| c == '`').count();
        ticks >= fence_len && last_line[ticks..].trim().is_empty() && ticks > 0
    };
    let body = if closes { &rest[..last_line_at] } else { rest };
    let mut code = body.to_string();
    if code.ends_with('\n') {
        code.pop();
    }
    let content_span = Span::new(body_start, body_start + body.len());

    let mut element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(Span::new(start, start + text.len()))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, content_span))],
            content_span,
        ))]);
    if !lang.is_empty() {
        element.data.args = Some(Value::Map(vec![("lang".to_string(), Value::String(lang))]));
    }
    Ok(Block::Element(element))
}
