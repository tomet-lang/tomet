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

use crate::dialect::Dialect;
use crate::element::{Groups, parse_hash_id, parse_paren_value, parse_value_group};
use crate::error::{Error, Result};
use crate::inline::{split_softbreaks, trim_edges};
use crate::value::eat_name;
use tomet_ast::{
    Block, Connect, Document, Element, Inline, Name, Paragraph, Placement, RawText, Section, Span,
    Value,
};
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};

/// The dialect lowering reads in. Patterns come with holes.
const DIALECT: Dialect = Dialect::Document;

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
            // The brackets of a `[content]` group are not content.
            SyntaxElement::Token(t) if node.kind() == K::CONTENT && is_bracket(t.kind()) => {}
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
        K::BLOCK_ELEMENT => lower_block_element(src, node).map(Some),
        K::SECTION => lower_section(src, node).map(Some),
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

fn is_bracket(kind: K) -> bool {
    matches!(kind, K::L_BRACKET | K::R_BRACKET)
}

fn cursor_at(src: &str, at: usize) -> Cursor<'_> {
    let mut cur = Cursor::new(src);
    cur.set_pos(at);
    cur
}

/// Where `node` starts and ends, white space and all.
fn range_of(node: &SyntaxNode) -> (usize, usize) {
    let range = node.text_range();
    (usize::from(range.start()), usize::from(range.end()))
}

/// The `@name` or `:name` a sigil-like node starts with: the name, read as the
/// AST parser reads one at `at`.
fn name_at(src: &str, at: usize) -> Result<Name> {
    let mut cur = cursor_at(src, at);
    eat_name(&mut cur).ok_or_else(|| unsupported("a name that does not read"))
}

/// An element: `@name` and the groups that follow it.
fn lower_block_element(src: &str, node: &SyntaxNode) -> Result<Block> {
    let element = lower_element(src, node)?.with_placement(Placement::Block);
    Ok(Block::Element(element))
}

fn lower_element(src: &str, node: &SyntaxNode) -> Result<Element> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .ok_or_else(|| unsupported("an element without a sigil"))?;
    let (sigil_start, _) = range_of(&sigil);
    if sigil.children_with_tokens().next().map(|e| e.kind()) != Some(K::AT) {
        return Err(unsupported("a sigil that is not `@`"));
    }
    let name = name_at(src, sigil_start + 1)?;
    let read = lower_groups(src, node)?;
    // The element ends with its last group; the white space and comments the
    // node also holds are not part of it.
    let end = node.children().map(|n| range_of(&n).1).max().unwrap_or(sigil_start);
    Ok(Element {
        name,
        placement: Placement::Inline,
        data: read.data,
        content: read.content,
        id: read.id,
        connects: read.connects,
        span: Span::new(sigil_start, end),
    })
}

/// The groups of `node`: `(args)`, `{value}`, `[content]`, `#(id)` and the
/// `:connect`s, whatever order they were written in. `(args)` and `{value}`
/// are read by the AST parser's own value grammar, at the group's offset.
fn lower_groups(src: &str, node: &SyntaxNode) -> Result<Groups> {
    let mut read = Groups::default();
    for child in node.children() {
        let (start, end) = range_of(&child);
        let mut cur = cursor_at(src, start);
        match child.kind() {
            K::SIGIL | K::SECTION_HEADING => {}
            K::ARGS => {
                read.data.args = Some(parse_paren_value(DIALECT, &mut cur)?);
                expect_end(&cur, end, "an args group")?;
            }
            K::VALUE_DATA => {
                read.data.value = Some(parse_value_group(DIALECT, &mut cur)?);
                expect_end(&cur, end, "a value group")?;
            }
            K::CONTENT => read.content = Some(lower_content(src, &child)?),
            K::ID_GROUP => {
                read.id = Some(parse_hash_id(&mut cur)?);
                expect_end(&cur, end, "an id")?;
            }
            K::CONNECT => read.connects.push(lower_connect(src, &child)?),
            other => return Err(unsupported(format!("{other:?} in an element's groups"))),
        }
    }
    Ok(read)
}

/// The AST parser's reading of a group has to end where the CST's group does;
/// if it does not, the two parsers disagree about the group and lowering says so.
fn expect_end(cur: &Cursor, end: usize, what: &str) -> Result<()> {
    if cur.pos() == end {
        Ok(())
    } else {
        Err(unsupported(format!("{what} that the two parsers end differently")))
    }
}

/// `[content]`: the blocks between the brackets. A `|` run is not lowered yet.
fn lower_content(src: &str, node: &SyntaxNode) -> Result<Vec<Block>> {
    if node.children_with_tokens().any(|e| e.kind() == K::PIPE) {
        return Err(unsupported("a `|` run"));
    }
    lower_blocks(src, node)
}

/// `:name(args){value}[content]`. The nameless merge form (`:(x: 1)`) is not lowered yet.
fn lower_connect(src: &str, node: &SyntaxNode) -> Result<Connect> {
    let (start, _) = range_of(node);
    let Some(ident) = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .find(|t| t.kind() == K::IDENT)
    else {
        return Err(unsupported("a connect without a name"));
    };
    let name = name_at(src, usize::from(ident.text_range().start()))?;
    if node.children().any(|n| n.kind() == K::ID_GROUP) {
        return Err(unsupported("an id in a connect"));
    }
    let read = lower_groups(src, node)?;
    let end = node.children().map(|n| range_of(&n).1).max().unwrap_or(start + 1);
    Ok(Connect {
        name,
        data: read.data,
        content: read.content,
        span: Span::new(start, end),
    })
}

/// A section: its heading (level, title, groups) and the blocks under it.
///
/// Only the headings with no title, with a title of plain text, or with a
/// `[title]` of one plain paragraph are lowered; a title with an element in it
/// is not yet.
fn lower_section(src: &str, node: &SyntaxNode) -> Result<Block> {
    let heading = node
        .children()
        .find(|n| n.kind() == K::SECTION_HEADING)
        .ok_or_else(|| unsupported("a section without a heading"))?;
    let (start, end) = range_of(&heading);
    let level = heading
        .children_with_tokens()
        .take_while(|e| e.kind() == K::EQUAL)
        .count();
    let read = lower_groups(src, &heading)?;

    let title = if let Some(content) = read.content.clone() {
        title_from_content(content)?
    } else {
        sugar_title(src, &heading, level)?
    };
    let mut section = Section::new(level, title, Span::new(start, end));
    section.data = read.data;
    section.id = read.id;
    section.connects = read.connects;
    for child in node.children().filter(|n| n.kind() != K::SECTION_HEADING) {
        section.blocks.extend(lower_block(src, &child)?);
    }
    Ok(Block::Section(section))
}

/// The inline content of a `[title]`: it has to be one paragraph.
fn title_from_content(mut blocks: Vec<Block>) -> Result<Vec<Inline>> {
    match blocks.len() {
        0 => Ok(Vec::new()),
        1 => match blocks.pop() {
            Some(Block::Paragraph(p)) => Ok(p.content),
            _ => Err(unsupported("a title that is not a paragraph")),
        },
        _ => Err(unsupported("a title of several blocks")),
    }
}

/// `== text`: the heading's text, when it is plain.
fn sugar_title(src: &str, heading: &SyntaxNode, level: usize) -> Result<Vec<Inline>> {
    if heading.children().next().is_some() {
        return Err(unsupported("a sugar title with groups"));
    }
    if heading.children_with_tokens().any(|e| e.kind() == K::COMMENT) {
        return Err(unsupported("a comment in a heading"));
    }
    let (start, end) = range_of(heading);
    let text = &src[start..end];
    let text = text.trim_end_matches(['\n', '\r']);
    let after = text[level..].trim_start();
    if after.is_empty() {
        return Ok(Vec::new());
    }
    // The AST parser takes a trailing `(args)`, `{value}`, `#(id)` or `:connect`
    // off the end of a sugar title, and the CST does not know them there yet.
    // Until it does, a title that could end in one is not lowered.
    if after.contains(['(', '{', '#', ':']) {
        return Err(unsupported("a sugar title that may end in groups"));
    }
    let from = start + (text.len() - after.len());
    let mut items = Vec::new();
    split_softbreaks(&mut items, &src[from..start + text.len()], from, &[]);
    Ok(trim_edges(items))
}
