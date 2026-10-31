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
use crate::element::{
    Groups, is_document_element_start, opens_group, parse_declaration, parse_hash_id,
    parse_paren_value, parse_value_group,
};
use crate::error::{Error, Result};
use crate::section::{merge_values, trim_trailing_equals};
use crate::inline::{DELIMITERS, extend_merging, push_text, split_softbreaks, trim_edges};
use crate::interp::{is_interp_start, parse_interpolation};
use crate::value::{eat_name, err, skip_block_comment, skip_inline_ws};
use tomet_ast::{
    Block, Connect, Data, Declaration, Document, Element, Inline, List, Name, Paragraph, Placement, RawText, Section, Span,
    Value,
};
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new, list, list_item};

thread_local! {
    /// The dialect being lowered: a pattern has holes, a document has not. It is the
    /// state of one call to [`lower_document`] or [`lower_pattern`], read by the
    /// value grammar the groups are handed to.
    static DIALECT_IN_USE: std::cell::Cell<Dialect> = const { std::cell::Cell::new(Dialect::Document) };
}

fn dialect() -> Dialect {
    DIALECT_IN_USE.with(|d| d.get())
}

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
    lower_in(Dialect::Document, src, root)
}

/// Reads the CST of a pattern (from `parse_cst_pattern`) as an AST [`Document`],
/// holes included.
#[doc(hidden)]
pub fn lower_pattern(src: &str, root: &SyntaxNode) -> Result<Document> {
    lower_in(Dialect::Pattern, src, root)
}

fn lower_in(in_dialect: Dialect, src: &str, root: &SyntaxNode) -> Result<Document> {
    let before = DIALECT_IN_USE.with(|d| d.replace(in_dialect));
    let blocks = lower_blocks(src, root).map_err(|e| e.located_in(src));
    DIALECT_IN_USE.with(|d| d.set(before));
    Ok(Document::new(blocks?, Span::new(0, src.len())))
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
            SyntaxElement::Token(t) if t.kind().is_trivia() || t.kind() == K::SEMI => {
                if t.kind() == K::COMMENT {
                    reject_unclosed_comment(src, usize::from(t.text_range().start()))?;
                }
            }
            // The brackets of a `[content]` group are not content.
            SyntaxElement::Token(t) if node.kind() == K::CONTENT && is_bracket(t.kind()) => {}
            // The `|` that opens a run, and the one that begins each of its lines.
            SyntaxElement::Token(t) if node.kind() == K::CONTENT && t.kind() == K::PIPE => {}
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
        K::LIST => lower_list(src, node).map(|l| Some(Block::List(l))),
        K::THEMATIC_BREAK => lower_thematic_break(src, node).map(Some),
        K::CODE_BLOCK => lower_code_block(src, node).map(Some),
        other => Err(unsupported(format!("node {other:?}"))),
    }
}

/// A paragraph: running text, with the elements, spans and interpolations in it.
fn lower_paragraph(src: &str, node: &SyntaxNode) -> Result<Option<Block>> {
    // From the first character that is not white space to the end of the last
    // line: the node also holds the newline that ends the paragraph.
    let (node_start, node_end) = range_of(node);
    let text = &src[node_start..node_end];
    let lead = text.len() - text.trim_start().len();
    let start = node_start + lead;
    let mut end = node_start + text.trim_end_matches(['\n', '\r']).len();
    if start >= end {
        return Ok(None);
    }
    // The AST parser's paragraph runs to where it stopped reading: past the blanks
    // before the line break or the `]` that closes the group it stands in.
    end += src[end..].len() - src[end..].trim_start_matches([' ', '\t']).len();
    if let Some(parent) = node.parent().filter(|p| p.kind() == K::CONTENT) {
        let beyond = &src[end..];
        let close = end + (beyond.len() - beyond.trim_start().len());
        if parent
            .children_with_tokens()
            .any(|e| e.kind() == K::R_BRACKET && usize::from(e.text_range().start()) == close)
        {
            end = close;
        }
    }
    // A `::` that opens a block is a declaration, which stands alone on its line.
    if pipe_depth(node) == 0 && is_document_element_start(&cursor_at(src, start)) {
        let mut cur = cursor_at(src, start);
        parse_declaration(dialect(), &mut cur, &[])?;
        skip_inline_ws(&mut cur);
        return Err(err(cur.pos(), "a `::` declaration stands alone on its line"));
    }
    let mut content = lower_inlines(src, node, start, end)?;
    if content.is_empty() {
        return Ok(None);
    }
    // A `${...}` that is the whole of its line, with nothing folded into it, is
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

/// The running text in `src[from..to]`, whose elements, spans and
/// interpolations are the children of `parent`: the text between them becomes
/// `Text` and `SoftBreak`, a comment is left out, and the ends are trimmed.
fn lower_inlines(src: &str, parent: &SyntaxNode, from: usize, to: usize) -> Result<Vec<Inline>> {
    let depth = pipe_depth(parent);
    let mut items = Vec::new();
    let mut text_start = from;
    for child in parent.children_with_tokens() {
        let range = child.text_range();
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        if start < from || end > to {
            continue;
        }
        match child {
            SyntaxElement::Token(t) if t.kind() == K::COMMENT => {
                reject_unclosed_comment(src, start)?;
                push_text_run(src, &mut items, text_start, start, depth)?;
                text_start = end;
            }
            SyntaxElement::Token(_) => {}
            SyntaxElement::Node(n) => {
                push_text_run(src, &mut items, text_start, start, depth)?;
                text_start = end;
                items.push(lower_inline(src, &n)?);
            }
        }
    }
    push_text_run(src, &mut items, text_start, to, depth)?;
    Ok(trim_edges(items))
}

/// `src[from..to]` as text. A `\;` that opens a line is a `;` that is text:
/// the backslash is not part of it.
fn push_text_run(src: &str, items: &mut Vec<Inline>, from: usize, to: usize, depth: usize) -> Result<()> {
    if from >= to {
        return Ok(());
    }
    reject_broken_constructs(src, from, to)?;
    let cols = vec![0usize; depth];
    let mut at = from;
    for backslash in escaped_semicolons(src, from, to) {
        split_softbreaks(items, &src[at..backslash], at, &cols);
        at = backslash + 1;
    }
    split_softbreaks(items, &src[at..to], at, &cols);
    Ok(())
}

/// One inline node of running text.
fn lower_inline(src: &str, node: &SyntaxNode) -> Result<Inline> {
    match node.kind() {
        K::INLINE_ELEMENT => lower_inline_element(src, node),
        K::EMPHASIS | K::STRONG | K::STRIKEOUT => lower_delimited(src, node).map(Inline::Element),
        K::CODE_SPAN => lower_code_span(src, node).map(Inline::Element),
        K::AUTOLINK => Ok(Inline::Element(lower_autolink(src, node))),
        other => Err(unsupported(format!("inline node {other:?}"))),
    }
}

/// `@name(...)[...]` in running text, or a `${...}` interpolation.
fn lower_inline_element(src: &str, node: &SyntaxNode) -> Result<Inline> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .and_then(|n| n.first_token())
        .map(|t| t.kind());
    match sigil {
        Some(K::AT) => lower_element(src, node).map(Inline::Element),
        Some(K::QUESTION) => lower_hole(src, node).map(Inline::Hole),
        Some(K::DOLLAR) => {
            let (start, end) = range_of(node);
            let mut cur = cursor_at(src, start);
            let interp = parse_interpolation(&mut cur)?;
            expect_end(&cur, end, "an interpolation")?;
            Ok(Inline::Interp(interp))
        }
        _ => Err(unsupported("an inline element with this sigil")),
    }
}

/// `*em*`, `**strong**`, `~~strikeout~~` and the `_` spellings: an element whose
/// content is one paragraph, the text between the delimiters.
fn lower_delimited(src: &str, node: &SyntaxNode) -> Result<Element> {
    let (start, end) = range_of(node);
    let text = &src[start..end];
    let (delimiter, name) = DELIMITERS
        .iter()
        .find(|(d, _)| text.starts_with(d) && text.ends_with(d) && text.len() >= 2 * d.len())
        .ok_or_else(|| unsupported("a delimited span that does not read"))?;
    let inner = lower_inlines(src, node, start + delimiter.len(), end - delimiter.len())?;
    let span = Span::new(start, end);
    Ok(element_new(Name::bare(*name))
        .with_span(span)
        .with_content(vec![Block::Paragraph(Paragraph::new(inner, span))]))
}

/// A code span: a `raw` element whose content is the text between the backticks.
fn lower_code_span(src: &str, node: &SyntaxNode) -> Result<Element> {
    let (start, end) = range_of(node);
    let text = &src[start..end];
    let fence = text.chars().take_while(|&c| c == '`').count();
    if fence == 0 || text.len() < 2 * fence {
        return Err(unsupported("a code span that does not read"));
    }
    let inner = &text[fence..text.len() - fence];
    let inner_span = Span::new(start + fence, end - fence);
    Ok(element_new(Name::bare("raw"))
        .with_placement(Placement::Inline)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(inner, inner_span))],
            inner_span,
        ))]))
}

/// A URL in running text: a `link` element whose `target` is the URL.
fn lower_autolink(src: &str, node: &SyntaxNode) -> Element {
    let (start, end) = range_of(node);
    let mut element = element_new(Name::bare("link"))
        .with_placement(Placement::Inline)
        .with_span(Span::new(start, end));
    element.data = Data {
        args: Some(Value::Map(vec![(
            "target".to_string(),
            Value::String(src[start..end].to_string()),
        )])),
        value: None,
    };
    element
}

/// The source text of `node` and where it starts.
fn text_of<'a>(src: &'a str, node: &SyntaxNode) -> (usize, &'a str) {
    let range = node.text_range();
    let (start, end) = (usize::from(range.start()), usize::from(range.end()));
    (start, &src[start..end])
}

/// `---`: a run of dashes on a line of its own; or `---[ title ]---`, whose
/// title is the content of the `hr` element.
fn lower_thematic_break(src: &str, node: &SyntaxNode) -> Result<Block> {
    if node.children_with_tokens().any(|e| e.kind() == K::COMMENT) {
        return Err(unsupported("a comment after a thematic break"));
    }
    let (start, end) = range_of(node);
    if let Some(content) = node.children().find(|n| n.kind() == K::CONTENT) {
        // Read from just after the `[`, so the white space before the text is in
        // the span of its `Text`, as the AST parser has it.
        let (open, close) = range_of(&content);
        reject_unterminated(src, &content)?;
        // After the `]`: three or more dashes, and nothing else on the line.
        let mut cur = cursor_at(src, close);
        skip_inline_ws(&mut cur);
        if cur.eat_while(|c| c == '-').len() < 3 {
            return Err(err(
                cur.pos(),
                "expected 3 or more '-' to close the titled thematic break",
            ));
        }
        skip_inline_ws(&mut cur);
        if !matches!(cur.peek(), None | Some('\n') | Some('\r')) {
            return Err(err(
                cur.pos(),
                "unexpected trailing content after titled thematic break",
            ));
        }
        let title = match content.children().find(|n| n.kind() == K::PARAGRAPH) {
            Some(paragraph) if content.children().count() == 1 => {
                lower_inlines(src, &paragraph, open + 1, close - 1)?
            }
            None => Vec::new(),
            Some(_) => return Err(unsupported("a title of several blocks")),
        };
        // The span of a titled break runs through its line end.
        let span = Span::new(start, end);
        let mut hr = element_new(Name::bare("hr"))
            .with_placement(Placement::Block)
            .with_span(span);
        hr.content = Some(vec![Block::Paragraph(Paragraph::new(title, span))]);
        return Ok(Block::Element(hr));
    }
    if node.children().next().is_some() {
        return Err(unsupported("a thematic break with something in it"));
    }
    let end = start + src[start..end].trim_end_matches(['\n', '\r']).len();
    Ok(Block::Element(
        element_new(Name::bare("hr"))
            .with_placement(Placement::Block)
            .with_span(Span::new(start, end)),
    ))
}

/// A fenced code block: a `raw` element whose content is the body, verbatim,
/// and whose `lang` is the first word of the line the fence opens on.
fn lower_code_block(src: &str, node: &SyntaxNode) -> Result<Block> {
    let (start, text) = text_of(src, node);
    let fence_len = text.chars().take_while(|&c| c == '`').count();
    let (opening, rest) = match text.find('\n') {
        Some(n) => (&text[fence_len..n], &text[n + 1..]),
        None => (&text[fence_len..], ""),
    };
    let lang = opening.split_whitespace().next().unwrap_or("").to_string();
    let body_start = start + (text.len() - rest.len());
    // In a `|` run when a run holds it -- not when a `|` is part of the code.
    if pipe_depth(node) > 0 {
        return lower_code_block_in_run(node, start, text, fence_len, &lang, body_start);
    }

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

/// A block element: `@name` and the groups that follow it, or `::name` and
/// its groups, a declaration about the document.
fn lower_block_element(src: &str, node: &SyntaxNode) -> Result<Block> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .and_then(|n| n.first_token())
        .map(|t| t.kind());
    if sigil == Some(K::COLON) {
        return lower_declaration(src, node).map(Block::Declaration);
    }
    if sigil == Some(K::QUESTION) {
        let mut hole = lower_hole(src, node)?;
        hole.placement = Placement::Block;
        return Ok(Block::Hole(hole));
    }
    let element = lower_element(src, node)?.with_placement(Placement::Block);
    Ok(Block::Element(element))
}

/// `::name(args){value}[content]#(id)`.
fn lower_declaration(src: &str, node: &SyntaxNode) -> Result<Declaration> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .ok_or_else(|| unsupported("a declaration without a sigil"))?;
    let (start, _) = range_of(&sigil);
    let name = name_at(src, start + 2)?;
    let read = lower_groups(src, node)?;
    if !read.connects.is_empty() {
        return Err(err(
            start,
            "a `::` declaration takes no `:connect`: a connect belongs to an element",
        ));
    }
    let end = node.children().map(|n| range_of(&n).1).max().unwrap_or(start);
    reject_second_group(src, end)?;
    Ok(Declaration {
        name,
        data: read.data,
        content: read.content,
        id: read.id,
        span: Span::new(start, end),
    })
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
    reject_second_group(src, end)?;
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
            // What a list item holds besides groups: its sublist and the inline
            // nodes of its text, lowered by the item itself.
            K::LIST
            | K::INLINE_ELEMENT
            | K::EMPHASIS
            | K::STRONG
            | K::STRIKEOUT
            | K::CODE_SPAN
            | K::AUTOLINK => {}
            K::ARGS => {
                read.data.args = Some(parse_paren_value(dialect(), &mut cur)?);
                expect_end(&cur, end, "an args group")?;
            }
            K::VALUE_DATA => {
                read.data.value = Some(parse_value_group(dialect(), &mut cur)?);
                expect_end(&cur, end, "a value group")?;
            }
            K::CONTENT => read.content = Some(lower_content(src, &child)?),
            K::ID_GROUP => {
                read.id = Some(parse_hash_id(&mut cur)?);
                expect_end(&cur, end, "an id")?;
            }
            K::CONNECT if is_merge_connect(&child) => merge_connect(src, &child, &mut read)?,
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
    reject_unterminated(src, node)?;
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
        return Err(err(
            start,
            "a `:connect` takes no `#(id)`: write the id of its owner before the first `:connect`",
        ));
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

/// `== text`: the heading's text, up to the groups it ends in.
fn sugar_title(src: &str, heading: &SyntaxNode, level: usize) -> Result<Vec<Inline>> {
    if heading.children_with_tokens().any(|e| e.kind() == K::COMMENT) {
        return Err(unsupported("a comment in a heading"));
    }
    let (start, end) = range_of(heading);
    let line_end = start + src[start..end].trim_end_matches(['\n', '\r']).len();
    let after = src[start + level..line_end].trim_start();
    if after.is_empty() {
        return Ok(Vec::new());
    }
    let from = line_end - after.len();
    // The text stops where the first of its trailing groups starts.
    let to = heading
        .children()
        .filter(|n| matches!(n.kind(), K::ARGS | K::VALUE_DATA | K::ID_GROUP | K::CONNECT))
        .map(|n| range_of(&n).0)
        .filter(|&at| at >= from)
        .min()
        .unwrap_or(line_end);
    let mut title = lower_inlines(src, heading, from, to)?;
    trim_trailing_equals(&mut title);
    Ok(title)
}

/// A list: its items, each with the sublist that follows it.
fn lower_list(src: &str, node: &SyntaxNode) -> Result<List> {
    let items = node
        .children()
        .filter(|n| n.kind() == K::LIST_ITEM)
        .map(|item| lower_list_item(src, &item))
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

/// One item. Its shapes: `- text`, `- (marker) text`, the combine notation
/// `-@name(...)[...] text`, and the full form, where a group opens on the marker
/// (`- [ content ] text`). The groups before the text are the marker's and the
/// full form's; those after it are the ones a sugar body ends in.
fn lower_list_item(src: &str, node: &SyntaxNode) -> Result<tomet_ast::ListItem> {
    let (start, _) = range_of(node);
    let sublist = match node.children().find(|n| n.kind() == K::LIST) {
        Some(sub) => Some(lower_list(src, &sub)?),
        None => None,
    };
    let marker_end = start
        + 1
        + usize::from(
            node.children_with_tokens()
                .nth(1)
                .is_some_and(|e| e.kind() == K::DOT),
        );
    let is_group = |n: &SyntaxNode| {
        matches!(
            n.kind(),
            K::ARGS | K::VALUE_DATA | K::CONTENT | K::ID_GROUP | K::CONNECT
        )
    };

    // Walk the item's own children: the groups before its text, its text, and the
    // groups after it.
    let mut combine = None;
    let mut leading: Vec<SyntaxNode> = Vec::new();
    let mut trailing: Vec<SyntaxNode> = Vec::new();
    let mut text_start = None;
    for child in node.children_with_tokens().skip_while(|e| {
        matches!(e, SyntaxElement::Token(t) if matches!(t.kind(), K::MINUS | K::DOT))
    }) {
        match child {
            SyntaxElement::Token(t) if t.kind() == K::WHITESPACE => {}
            SyntaxElement::Node(n) if n.kind() == K::LIST => break,
            SyntaxElement::Node(n) if is_group(&n) => {
                if text_start.is_none() {
                    leading.push(n);
                } else {
                    trailing.push(n);
                }
            }
            SyntaxElement::Node(n)
                if n.kind() == K::INLINE_ELEMENT
                    && range_of(&n).0 == marker_end
                    && text_start.is_none() =>
            {
                combine = Some(n);
            }
            other => {
                if text_start.is_none() {
                    text_start = Some(usize::from(other.text_range().start()));
                }
            }
        }
    }

    // The end of the item's own line: past its groups, which may span lines.
    let limit = sublist_start(node).unwrap_or_else(|| range_of(node).1);
    let line_end = {
        let from = node
            .children()
            .filter(|n| n.kind() != K::LIST)
            .map(|n| range_of(&n).1)
            .max()
            .unwrap_or(marker_end)
            .min(limit);
        let within = &src[from..limit];
        from + within.find(['\n', '\r']).unwrap_or(within.len())
    };
    // The AST parser's item runs through the line break, and through the sublist
    // when it has one.
    let span_end = match &sublist {
        Some(sub) => sub.span.end,
        None if src[line_end..].starts_with('\n') => line_end + 1,
        None => line_end,
    };
    let line_start = src[..start].rfind('\n').map_or(0, |n| n + 1);
    let span_start = if src[line_start..start].chars().all(char::is_whitespace) {
        line_start
    } else {
        start
    };
    let span = Span::new(span_start, span_end);

    let mut item = if let Some(element) = combine {
        if !leading.is_empty() || !trailing.is_empty() {
            return Err(unsupported("a combine item with other groups"));
        }
        let el = lower_element(src, &element)?;
        let mut content = el.content.unwrap_or_default();
        append_trailing_text(src, node, &mut content, range_of(&element).1, line_end)?;
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
        let read = lower_groups(src, node)?;
        let full_form = leading.iter().any(|n| n.kind() != K::ARGS);
        let attrs = read.data.value.map(|v| v.as_data());
        let content = if full_form {
            let mut blocks = read.content.unwrap_or_default();
            let after = leading.iter().map(|n| range_of(n).1).max().unwrap_or(marker_end);
            append_trailing_text(src, node, &mut blocks, after, line_end)?;
            blocks
        } else {
            // The text, up to the groups it ends in.
            let to = trailing.first().map_or(line_end, |n| range_of(n).0);
            match text_start {
                Some(from) if from < to => {
                    let inlines = lower_inlines(src, node, from, to)?;
                    if inlines.is_empty() {
                        Vec::new()
                    } else {
                        vec![Block::Paragraph(Paragraph::new(inlines, Span::new(from, line_end)))]
                    }
                }
                _ => Vec::new(),
            }
        };
        list_item(None, content, read.data.args, attrs, read.id, read.connects, None, span)
    };
    item.sublist = sublist;
    Ok(item)
}

fn sublist_start(node: &SyntaxNode) -> Option<usize> {
    node.children()
        .find(|n| n.kind() == K::LIST)
        .map(|sub| range_of(&sub).0)
}

/// What follows the groups of a full-form or combine item on its line is added
/// to the last paragraph of its content, with a space where the source had one.
fn append_trailing_text(
    src: &str,
    node: &SyntaxNode,
    content: &mut Vec<Block>,
    after_groups: usize,
    line_end: usize,
) -> Result<()> {
    let body = &src[after_groups..line_end];
    let lead = body.len() - body.trim_start().len();
    let start = after_groups + lead;
    if start >= line_end {
        return Ok(());
    }
    let rest = lower_inlines(src, node, start, line_end)?;
    if rest.is_empty() {
        return Ok(());
    }
    if lead > 0
        && let Some(Block::Paragraph(p)) = content.last_mut()
    {
        push_text(&mut p.content, " ".to_string(), Span::new(after_groups, start));
    }
    match content.last_mut() {
        Some(Block::Paragraph(p)) => extend_merging(&mut p.content, rest),
        _ => content.push(Block::Paragraph(Paragraph::new(rest, Span::new(start, line_end)))),
    }
    Ok(())
}

/// `:(args)` or `:{value}`: no name, so it is not a connect of its own but more
/// of the owner's args or value.
fn is_merge_connect(node: &SyntaxNode) -> bool {
    !node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .any(|t| t.kind() == K::IDENT)
}

/// Merges the groups of a nameless `:` connect into `read`, as the AST parser does.
fn merge_connect(src: &str, node: &SyntaxNode, read: &mut Groups) -> Result<()> {
    for child in node.children() {
        let (start, end) = range_of(&child);
        let mut cur = cursor_at(src, start);
        match child.kind() {
            K::ARGS => {
                let args = parse_paren_value(dialect(), &mut cur)?;
                expect_end(&cur, end, "an args group")?;
                read.data.args = merge_values(read.data.args.as_ref(), Some(&args));
            }
            K::VALUE_DATA => {
                let value = parse_value_group(dialect(), &mut cur)?;
                expect_end(&cur, end, "a value group")?;
                let merged = match &read.data.value {
                    Some(existing) if !existing.has_children() && !value.has_children() => {
                        merge_values(Some(&existing.as_data()), Some(&value.as_data()))
                            .map(tomet_ast::Group::from_map)
                    }
                    _ => None,
                };
                read.data.value = Some(merged.unwrap_or(value));
            }
            other => return Err(unsupported(format!("{other:?} in a nameless connect"))),
        }
    }
    Ok(())
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

/// How many `|` runs `node` stands inside: each one folds the marker at the start
/// of a line away with the line break before it.
fn pipe_depth(node: &SyntaxNode) -> usize {
    node.ancestors()
        .filter(|a| {
            a.kind() == K::CONTENT
                && a.children_with_tokens()
                    .any(|e| e.kind() == K::PIPE && e.as_token().is_some())
        })
        .count()
}

/// A fenced code block in a `|` run: each line of it begins with the markers of the
/// run, which are not part of the code, nor is the one space after the last.
fn lower_code_block_in_run(
    node: &SyntaxNode,
    start: usize,
    text: &str,
    fence_len: usize,
    lang: &str,
    body_start: usize,
) -> Result<Block> {
    let depth = pipe_depth(node);
    let rest = &text[body_start - start..];
    let mut code = String::new();
    for line in rest.split_inclusive('\n') {
        let mut after_markers = line;
        for _ in 0..depth {
            after_markers = after_markers.trim_start_matches([' ', '\t']);
            match after_markers.strip_prefix('|') {
                Some(r) => after_markers = r,
                None => return Err(unsupported("a fence line without the run's marker")),
            }
        }
        // A closing line: a run of at least as many backticks, then blanks.
        let closing = after_markers.trim_start_matches([' ', '\t']);
        let ticks = closing.chars().take_while(|&c| c == '`').count();
        if ticks >= fence_len && closing[ticks..].trim().is_empty() {
            break;
        }
        code.push_str(after_markers.strip_prefix(' ').unwrap_or(after_markers).trim_end_matches('\n'));
        if line.ends_with('\n') {
            code.push('\n');
        }
    }
    if code.ends_with('\n') {
        code.pop();
    }
    // The AST parser's body runs through the closing line when it is in a run.
    let end = start + text.len();
    let content_span = Span::new(body_start, end);
    let mut element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, content_span))],
            content_span,
        ))]);
    if !lang.is_empty() {
        element.data.args = Some(Value::Map(vec![("lang".to_string(), Value::String(lang.to_string()))]));
    }
    Ok(Block::Element(element))
}

/// A group written against the end of an element, when the element has one of
/// that kind already, is an error: the CST left it as text, and the AST parser
/// refuses it. `:` in front of it merges into the one that is there.
fn reject_second_group(src: &str, end: usize) -> Result<()> {
    let cur = cursor_at(src, end);
    if !opens_group(&cur) {
        return Ok(());
    }
    if cur.peek() == Some('#') {
        return Err(err(end, "a second `#(...)`: an element takes at most one id"));
    }
    let group = match cur.peek() {
        Some('(') => "(args)",
        Some('[') | Some('|') => "[content]",
        _ => "{value}",
    };
    Err(err(
        end,
        format!(
            "a second `{group}` group: an element takes one of each. \
             Write `:` in front of it to merge into the one already there"
        ),
    ))
}

/// A `[content]` group that never closes.
fn reject_unterminated(src: &str, content: &SyntaxNode) -> Result<()> {
    let bracketed = content
        .children_with_tokens()
        .next()
        .is_some_and(|e| e.kind() == K::L_BRACKET);
    let closed = content
        .children_with_tokens()
        .last()
        .is_some_and(|e| e.kind() == K::R_BRACKET);
    if bracketed && !closed {
        return Err(err(src.len(), "unterminated, expected ']'"));
    }
    Ok(())
}

/// What the CST left as text that the AST parser would have read as a construct
/// and found broken: a `${` that does not close, a `/*` that does not close.
fn reject_broken_constructs(src: &str, from: usize, to: usize) -> Result<()> {
    let text = &src[from..to];
    for (i, _) in text.match_indices("${") {
        let mut cur = cursor_at(src, from + i);
        if is_interp_start(&cur) {
            parse_interpolation(&mut cur)?;
        }
    }
    for (i, _) in text.match_indices("/*") {
        let mut cur = cursor_at(src, from + i);
        skip_block_comment(&mut cur)?;
    }
    Ok(())
}

/// A `/*` comment has to close.
fn reject_unclosed_comment(src: &str, at: usize) -> Result<()> {
    if src[at..].starts_with("/*") {
        skip_block_comment(&mut cursor_at(src, at))?;
    }
    Ok(())
}

/// `?matcher(args){value}[content]#(id)`, in a pattern.
fn lower_hole(src: &str, node: &SyntaxNode) -> Result<tomet_ast::Hole> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .ok_or_else(|| unsupported("a hole without a sigil"))?;
    let (start, sigil_end) = range_of(&sigil);
    let matcher = if sigil_end > start + 1 {
        Some(name_at(src, start + 1)?)
    } else {
        None
    };
    let read = lower_groups(src, node)?;
    let end = node.children().map(|n| range_of(&n).1).max().unwrap_or(sigil_end);
    reject_second_group(src, end)?;
    Ok(tomet_ast::Hole {
        matcher,
        placement: Placement::Inline,
        data: read.data,
        content: read.content,
        id: read.id,
        span: Span::new(start, end),
    })
}
