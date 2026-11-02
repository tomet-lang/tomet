//! Lowering: the CST read as an AST.
//!
//! The CST parser is the one reader of the grammar's shape (where a block, an
//! element, a group starts and ends); the AST is the owned, typed tree the rest
//! of the system works on. Lowering is the fold from the first to the second, so
//! that a construct is written once, in the CST parser: `parse_document` is
//! `parse_cst` and then [`lower_document`].
//!
//! Lowering is also where a document is refused. The CST never fails, so what
//! the grammar does not allow -- a second `(args)` group, a `[` that never
//! closes, a `|` in a column that continues nothing -- is found here, with a
//! message and the offset it is at.
//!
//! What it reads from text rather than from nodes is not shape: how a run of
//! source text becomes `Text` and `SoftBreak` items
//! ([`crate::inline::split_softbreaks`]), and the grammars of the leaves --
//! `${...}` expressions and a group's values, which hand back to the CST
//! whatever they embed (see [`crate::value::Embedded`]).

use crate::element::Groups;
use crate::error::{Error, Result};
use crate::inline::{DELIMITERS, extend_merging, push_soft_break, push_text, trim_edges};
use crate::value::err;
use tomet_ast::{
    Block, Connect, Data, Declaration, Document, Element, Inline, List, Name, Paragraph, Placement,
    RawText, Section, Value,
};
use tomet_cst::{
    AstNode, CstCodeBlock, CstName, NodeOrToken, SyntaxElement, SyntaxKind as K, SyntaxNode,
    SyntaxToken,
};
use tomet_span::Span;
use tomet_tree::{ElementExt, element_new, list, list_item};

/// The error for a tree lowering cannot read: a node the CST should not have put
/// there. It is a bug in the CST parser or in lowering, not in the document.
fn unsupported(what: impl std::fmt::Display) -> Error {
    Error {
        message: format!("the syntax tree holds something lowering does not read: {what}"),
        line: 0,
        column: 0,
        offset: 0,
    }
}

/// Reads the CST root as an AST [`Document`].
pub fn lower_document(root: &SyntaxNode) -> Result<Document> {
    reject_too_deep(root)?;
    let blocks = lower_blocks(root)?;
    Ok(Document::new(
        blocks,
        Span::new(0, usize::from(root.text_range().len())),
    ))
}

/// The error for a tree that holds a `TOO_DEEP` node: the parser stopped reading structure there, so
/// nothing below it is meaningful. One walk of the tree, once.
pub(crate) fn reject_too_deep(root: &SyntaxNode) -> Result<()> {
    match root.descendants().find(|n| n.kind() == K::TOO_DEEP) {
        Some(node) => Err(err(
            range_of(&node).0,
            format!(
                "this is nested more than {} levels deep",
                crate::cst::MAX_DEPTH
            ),
        )),
        None => Ok(()),
    }
}

/// The blocks that are children of `node`: the document, and (later) a section
/// or a `[content]` group.
fn lower_blocks(node: &SyntaxNode) -> Result<Vec<Block>> {
    let mut blocks = Vec::new();
    for child in node.children_with_tokens() {
        match child {
            // Trivia, and the `;` that only breaks a block, leave nothing behind.
            SyntaxElement::Token(t) if t.kind().is_trivia() || t.kind() == K::SEMI => {}
            // The brackets of a `[content]` group are not content.
            SyntaxElement::Token(t) if node.kind() == K::CONTENT && is_bracket(t.kind()) => {}
            // The `|` that opens a run.
            SyntaxElement::Token(t) if node.kind() == K::CONTENT && t.kind() == K::PIPE => {}
            // The `|` that begins each later line of a run.
            SyntaxElement::Token(t) if t.kind() == K::MARGIN => {}
            SyntaxElement::Token(t) => return Err(unsupported(format!("token {:?}", t.kind()))),
            SyntaxElement::Node(n) if n.kind() == K::ERROR => return Err(lower_error(&n)),
            SyntaxElement::Node(n) => blocks.extend(lower_block(&n)?),
        }
    }
    Ok(blocks)
}

/// One block node. `None` is a block that leaves nothing in the AST (an empty paragraph).
fn lower_block(node: &SyntaxNode) -> Result<Option<Block>> {
    match node.kind() {
        K::PARAGRAPH => lower_paragraph(node),
        K::BLOCK_ELEMENT => lower_block_element(node).map(Some),
        K::SECTION => lower_section(node).map(Some),
        K::LIST => lower_list(node).map(|l| Some(Block::List(l))),
        K::THEMATIC_BREAK => lower_thematic_break(node).map(Some),
        K::CODE_BLOCK => lower_code_block(node).map(Some),
        // The text after a declaration on its line, or an error node.
        K::ERROR => Err(lower_error(node)),
        other => Err(unsupported(format!("node {other:?}"))),
    }
}

/// The content range of a paragraph, computed from tree facts:
/// - starts at the first non-whitespace token or node
/// - ends before the trailing newline, or before the `]` when enclosed in a content group
fn paragraph_bounds(node: &SyntaxNode) -> Option<(usize, usize)> {
    let (_, node_end) = range_of(node);
    let first_content = node
        .children_with_tokens()
        .find(|e| match e {
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
fn lower_paragraph(node: &SyntaxNode) -> Result<Option<Block>> {
    let Some((start, end)) = paragraph_bounds(node) else {
        return Ok(None);
    };
    let mut content = lower_inlines(node, start, end)?;
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

/// The running text in `parent` between byte offsets `from` and `to`:
/// tokens become `Text` and `SoftBreak`, comments are skipped, `\;` becomes `;`,
/// and the ends are trimmed.
fn lower_inlines(parent: &SyntaxNode, from: usize, to: usize) -> Result<Vec<Inline>> {
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
                push_text(items, std::mem::take(pending_text), Span::new(start, *pending_end));
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
                flush_text(&mut items, &mut pending_text, &mut pending_start, &mut pending_end);
                flush_softbreak(&mut items, &mut in_softbreak, softbreak_start, softbreak_end);
                is_first_text = false;
            }
            SyntaxElement::Token(t) if t.kind() == K::NEWLINE => {
                flush_text(&mut items, &mut pending_text, &mut pending_start, &mut pending_end);
                if !in_softbreak {
                    in_softbreak = true;
                    softbreak_start = start;
                }
                softbreak_end = end;
                is_first_text = false;
            }
            SyntaxElement::Token(t) if in_softbreak && matches!(t.kind(), K::WHITESPACE | K::MARGIN) => {
                softbreak_end = end;
            }
            SyntaxElement::Token(t) => {
                flush_softbreak(&mut items, &mut in_softbreak, softbreak_start, softbreak_end);
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
                flush_softbreak(&mut items, &mut in_softbreak, softbreak_start, softbreak_end);
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
                flush_softbreak(&mut items, &mut in_softbreak, softbreak_start, softbreak_end);
                // An escape like `\;` at line start: the `\` is omitted, `;` is text.
                // The span starts after `\` (at start + 1) through end.
                let semi_start = start + 1;
                let effective_start = if is_first_text && from < semi_start {
                    from
                } else {
                    semi_start
                };
                is_first_text = false;
                pending_start.get_or_insert(effective_start);
                pending_text.push(';');
                pending_end = end;
            }
            SyntaxElement::Node(n) if n.kind() == K::ERROR => {
                return Err(lower_error(&n));
            }
            SyntaxElement::Node(n) => {
                flush_softbreak(&mut items, &mut in_softbreak, softbreak_start, softbreak_end);
                flush_text(&mut items, &mut pending_text, &mut pending_start, &mut pending_end);
                is_first_text = false;
                items.push(lower_inline(&n)?);
            }
        }
    }
    if pending_start.is_some() && to > pending_end {
        pending_end = to;
    }
    flush_text(&mut items, &mut pending_text, &mut pending_start, &mut pending_end);
    flush_softbreak(&mut items, &mut in_softbreak, softbreak_start, softbreak_end);
    Ok(trim_edges(items))
}

/// One inline node of running text.
fn lower_inline(node: &SyntaxNode) -> Result<Inline> {
    match node.kind() {
        K::INLINE_ELEMENT => lower_inline_element(node),
        K::EMPHASIS | K::STRONG | K::STRIKEOUT => lower_delimited(node).map(Inline::Element),
        K::CODE_SPAN => lower_code_span(node).map(Inline::Element),
        K::AUTOLINK => Ok(Inline::Element(lower_autolink(node))),
        K::ERROR => Err(lower_error(node)),
        other => Err(unsupported(format!("inline node {other:?}"))),
    }
}

/// `@name(...)[...]` in running text, or a `${...}` interpolation.
fn lower_inline_element(node: &SyntaxNode) -> Result<Inline> {
    if let Some(slot) = slot_of(node) {
        return lower_slot(&slot).map(Inline::Hole);
    }
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .and_then(|n| n.first_token())
        .map(|t| t.kind());
    match sigil {
        Some(K::AT) => lower_element(node).map(Inline::Element),
        Some(K::DOLLAR) => {
            let interp = crate::interp::lower_interpolation(node)?;
            Ok(Inline::Interp(interp))
        }
        _ => Err(unsupported("an inline element with this sigil")),
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

/// A code span: a `raw` element whose content is the text between the backticks.
fn lower_code_span(node: &SyntaxNode) -> Result<Element> {
    let (start, end) = range_of(node);
    let fence = node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .take_while(|t| t.kind() == K::BACKTICK)
        .map(|t| t.text().len())
        .sum::<usize>();
    if fence == 0 || (end - start) < 2 * fence {
        return Err(unsupported("a code span that does not read"));
    }
    let inner_span = Span::new(start + fence, end - fence);
    let mut inner = String::new();
    for t in node
        .children_with_tokens()
        .filter_map(|e| e.into_token())
        .skip_while(|t| t.kind() == K::BACKTICK)
        .take_while(|t| usize::from(t.text_range().end()) <= (end - fence))
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


/// `---`: a run of dashes on a line of its own; or `---[ title ]---`, whose
/// title is the content of the `hr` element.
fn lower_thematic_break(node: &SyntaxNode) -> Result<Block> {
    if node.children_with_tokens().any(|e| e.kind() == K::COMMENT) {
        return Err(unsupported("a comment after a thematic break"));
    }
    let (start, end) = range_of(node);
    if let Some(content) = node.children().find(|n| n.kind() == K::CONTENT) {
        // Read from just after the `[`, so the white space before the text is in
        // the span of its `Text`, as the AST parser has it.
        let (open, close) = range_of(&content);
        if let Some(err_node) = content.children().find(|c| c.kind() == K::ERROR) {
            return Err(lower_error(&err_node));
        }
        if let Some(err_node) = node.children().find(|c| c.kind() == K::ERROR) {
            return Err(lower_error(&err_node));
        }
        let title = match content.children().find(|n| n.kind() == K::PARAGRAPH) {
            Some(paragraph) if content.children().count() == 1 => {
                lower_inlines(&paragraph, open + 1, close - 1)?
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
    if node.children().any(|c| c.kind() != K::FENCE) {
        return Err(unsupported("a thematic break with something in it"));
    }
    let end = match node.last_token() {
        Some(token) if token.kind() == K::NEWLINE => usize::from(token.text_range().start()),
        _ => end,
    };
    Ok(Block::Element(
        element_new(Name::bare("hr"))
            .with_placement(Placement::Block)
            .with_span(Span::new(start, end)),
    ))
}

/// A fenced code block: a `raw` element whose content is the body, verbatim,
/// and whose `lang` is the first word of the line the fence opens on.
fn lower_code_block(node: &SyntaxNode) -> Result<Block> {
    let cst = CstCodeBlock::cast(node.clone()).expect("cast CstCodeBlock");
    if cst.is_line_raw() {
        return lower_line_raw(node);
    }
    let (start, end) = range_of(node);
    cst.opening_fence()
        .ok_or_else(|| unsupported("a code block without an opening fence"))?;
    let lang = cst
        .info_string()
        .and_then(|info| info.lang())
        .unwrap_or_default();

    let lines: Vec<SyntaxNode> = node.children().filter(|c| c.kind() == K::CODE_LINE).collect();
    let closing_fence = cst.closing_fence();

    let body_start = match lines.first() {
        Some(line) => usize::from(line.text_range().start()),
        None => match &closing_fence {
            Some(fence) => usize::from(fence.syntax().text_range().start()),
            None => end,
        },
    };

    if pipe_depth(node) > 0 {
        return lower_code_block_in_run(node, start, end, &lines, &lang, body_start);
    }

    let mut code = String::new();
    for line in &lines {
        use std::fmt::Write;
        write!(code, "{}", line.text()).unwrap();
    }
    if code.ends_with('\n') {
        code.pop();
    }
    let body_end = match lines.last() {
        Some(line) => usize::from(line.text_range().end()),
        None => body_start,
    };
    let content_span = Span::new(body_start, body_end);

    let mut element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, content_span))],
            content_span,
        ))]);
    if !lang.is_empty() {
        element.data.args = Some(Value::Map(vec![("lang".to_string(), Value::String(lang))]));
    }
    Ok(Block::Element(element))
}

/// A line raw: each line of the node is two backticks and the text after them,
/// less the one space that may follow the backticks. The lines are one `raw`
/// element, joined with a newline; the markers of a `|` run at the start of every
/// line but the first are not part of them.
fn lower_line_raw(node: &SyntaxNode) -> Result<Block> {
    let (start, end) = range_of(node);
    let depth = pipe_depth(node);
    let mut code = String::new();
    let mut content_start = None;
    let mut content_end = start;
    let lines = node.children().filter(|c| c.kind() == K::CODE_LINE);
    for (n, line) in lines.enumerate() {
        let (line_start, _) = range_of(&line);
        let req_markers = if n == 0 { 0 } else { depth };
        let mut marker_count = 0;
        let mut past_fence = false;
        let mut skipped_space = false;
        let mut body = String::new();
        let mut body_start = None;
        let mut fence_end = line_start;
        for elem in line.children_with_tokens() {
            match elem {
                NodeOrToken::Node(child) if child.kind() == K::FENCE => {
                    past_fence = true;
                    fence_end = usize::from(child.text_range().end());
                }
                NodeOrToken::Token(token) if !past_fence => {
                    if token.kind() == K::MARGIN {
                        marker_count += 1;
                    }
                }
                NodeOrToken::Token(token) if past_fence => {
                    if !skipped_space {
                        skipped_space = true;
                        if token.kind() == K::WHITESPACE && token.text().starts_with(' ') {
                            let text = token.text();
                            if text.len() > 1 {
                                let tok_start = usize::from(token.text_range().start()) + 1;
                                body_start.get_or_insert(tok_start);
                                body.push_str(&text[1..]);
                            }
                            continue;
                        }
                    }
                    if matches!(token.kind(), K::NEWLINE) {
                        continue;
                    }
                    body_start.get_or_insert(usize::from(token.text_range().start()));
                    body.push_str(token.text());
                }
                _ => {}
            }
        }
        if marker_count < req_markers {
            return Err(unsupported("a line raw without the run's marker"));
        }
        if !past_fence {
            return Err(unsupported("a line raw line without its backticks"));
        }
        let b_start = body_start.unwrap_or(if skipped_space { fence_end + 1 } else { fence_end });
        content_start.get_or_insert(b_start);
        content_end = b_start + body.len();
        if n > 0 {
            code.push('\n');
        }
        code.push_str(&body);
    }
    let content_span = Span::new(content_start.unwrap_or(start), content_end);
    let element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, content_span))],
            content_span,
        ))]);
    Ok(Block::Element(element))
}

fn is_bracket(kind: K) -> bool {
    matches!(kind, K::L_BRACKET | K::R_BRACKET)
}

/// Where `node` starts and ends, white space and all.
fn range_of(node: &SyntaxNode) -> (usize, usize) {
    let range = node.text_range();
    (usize::from(range.start()), usize::from(range.end()))
}

/// The name of an element, declaration, or connect, from its `NAME` child.
fn name_of(node: &SyntaxNode) -> Result<Name> {
    let name_node = node
        .children()
        .find(|n| n.kind() == K::NAME)
        .ok_or_else(|| unsupported("a name that does not read"))?;
    let cst_name = CstName::cast(name_node).expect("cast to CstName");
    let (namespace, name) = cst_name.parts();
    Ok(Name { namespace, name })
}

/// A block element: `@name` and the groups that follow it, or `::name` and
/// its groups, a declaration about the document.
fn lower_block_element(node: &SyntaxNode) -> Result<Block> {
    if let Some(slot) = slot_of(node) {
        let mut hole = lower_slot(&slot)?;
        hole.placement = Placement::Block;
        return Ok(Block::Hole(hole));
    }
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .and_then(|n| n.first_token())
        .map(|t| t.kind());
    if sigil == Some(K::COLON) {
        return lower_declaration(node).map(Block::Declaration);
    }
    let element = lower_element(node)?.with_placement(Placement::Block);
    Ok(Block::Element(element))
}

/// `::name(args){value}[content]#(id)`.
fn lower_declaration(node: &SyntaxNode) -> Result<Declaration> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .ok_or_else(|| unsupported("a declaration without a sigil"))?;
    let (start, _) = range_of(&sigil);
    let name = name_of(&sigil)?;
    let read = lower_groups(node)?;
    if !read.connects.is_empty() {
        return Err(err(
            start,
            "a `::` declaration takes no `:connect`: a connect belongs to an element",
        ));
    }
    let end = node
        .children()
        .map(|n| range_of(&n).1)
        .max()
        .unwrap_or(start);
    Ok(Declaration {
        name,
        data: read.data,
        content: read.content,
        id: read.id,
        span: Span::new(start, end),
    })
}

pub(crate) fn lower_element(node: &SyntaxNode) -> Result<Element> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .ok_or_else(|| unsupported("an element without a sigil"))?;
    let (sigil_start, _) = range_of(&sigil);
    if sigil.children_with_tokens().next().map(|e| e.kind()) != Some(K::AT) {
        return Err(unsupported("a sigil that is not `@`"));
    }
    let name = name_of(&sigil)?;
    let read = lower_groups(node)?;
    // The element ends with its last group; the white space and comments the
    // node also holds are not part of it.
    let end = node
        .children()
        .map(|n| range_of(&n).1)
        .max()
        .unwrap_or(sigil_start);
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
/// `:connect`s, whatever order they were written in.
fn lower_groups(node: &SyntaxNode) -> Result<Groups> {
    lower_group_list(node.children())
}

/// The groups in `nodes`, read as [`lower_groups`] reads an element's.
pub(crate) fn lower_group_list(
    nodes: impl IntoIterator<Item = SyntaxNode>,
) -> Result<Groups> {
    let mut read = Groups::default();
    for child in nodes {
        match child.kind() {
            K::SIGIL | K::SECTION_HEADING | K::NAME | K::BODY => {}
            // What a list item holds besides groups: its sublist and the inline
            // nodes of its text, lowered by the item itself.
            K::LIST
            | K::INLINE_ELEMENT
            | K::SLOT
            | K::EMPHASIS
            | K::STRONG
            | K::STRIKEOUT
            | K::CODE_SPAN
            | K::AUTOLINK => {}
            K::ARGS => {
                read.data.args = Some(crate::element::lower_paren_value(&child)?);
            }
            K::VALUE_DATA | K::MAP_LITERAL => {
                read.data.value = Some(crate::element::lower_value_group(&child)?);
            }
            K::CONTENT => read.content = Some(lower_content(&child)?),
            K::ID_GROUP => {
                read.id = Some(crate::element::lower_hash_id(&child)?);
            }
            K::CONNECT if is_merge_connect(&child) => merge_connect(&child, &mut read)?,
            K::CONNECT => read.connects.push(lower_connect(&child)?),
            K::ERROR => return Err(lower_error(&child)),
            other => return Err(unsupported(format!("{other:?} in an element's groups"))),
        }
    }
    Ok(read)
}

/// What a value embeds at `node`, and where it ends: an element or a hole
/// (`@name...`, `?matcher(...)`), or `[content]` / a `|` run as blocks.
pub(crate) fn lower_embedded(node: &SyntaxNode) -> Result<(Value, usize)> {
    match node.kind() {
        K::CONTENT => {
            let (_, end) = range_of(node);
            Ok((Value::Blocks(lower_content(node)?), end))
        }
        _ if slot_of(node).is_some() => {
            let slot = slot_of(node).expect("checked");
            let hole = lower_slot(&slot)?;
            let end = hole.span.end;
            Ok((Value::Hole(Box::new(hole)), end))
        }
        _ => {
            let element = lower_element(node)?;
            let end = element.span.end;
            Ok((Value::Element(Box::new(element)), end))
        }
    }
}

/// `[content]`: the blocks between the brackets, or the blocks of a `|` run.
pub(crate) fn lower_content(node: &SyntaxNode) -> Result<Vec<Block>> {
    if let Some(err_node) = node
        .children()
        .find(|c| c.kind() == K::ERROR && c.children_with_tokens().count() == 0)
    {
        return Err(lower_error(&err_node));
    }
    lower_blocks(node)
}

/// `:name(args){value}[content]`.
fn lower_connect(node: &SyntaxNode) -> Result<Connect> {
    let (start, _) = range_of(node);
    let name = name_of(node).map_err(|_| unsupported("a connect without a name"))?;
    if node.children().any(|n| n.kind() == K::ID_GROUP) {
        return Err(err(
            start,
            "a `:connect` takes no `#(id)`: write the id of its owner before the first `:connect`",
        ));
    }
    let read = lower_groups(node)?;
    let end = node
        .children()
        .map(|n| range_of(&n).1)
        .max()
        .unwrap_or(start + 1);
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
fn lower_section(node: &SyntaxNode) -> Result<Block> {
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
/// `${...}` (`=[ ${x} ]`) is demoted to inline and is the title's only node.
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
            Some(Block::Section(_)) => Err(only("a nested section")),
            Some(Block::List(_)) => Err(only("a list")),
            Some(Block::Declaration(_)) => Err(only("a `::` declaration")),
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
    trim_trailing_equals(&mut title);
    Ok(title)
}

/// A list: its items, each with the sublist that follows it.
fn lower_list(node: &SyntaxNode) -> Result<List> {
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
        None => {
            node.children_with_tokens()
                .filter_map(|e| e.into_token())
                .find(|t| t.kind() == K::NEWLINE && usize::from(t.text_range().start()) == line_end)
                .map_or(line_end, |t| usize::from(t.text_range().end()))
        }
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
                let only_blanks = prev_token.is_none() || prev_token.map(|t| t.kind()) == Some(K::NEWLINE);
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
    let first_content = body
        .children_with_tokens()
        .find(|e| match e {
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
    let combine = node
        .children()
        .find(|n| n.kind() == K::INLINE_ELEMENT);
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

/// Whether the groups in `read` hold anything: a `:name` with nothing of its own
/// is not a connect, only a word with a colon in front of it.
pub(crate) fn claims_something(read: &Groups) -> bool {
    read.data.args.is_some()
        || read.content.is_some()
        || read.data.value.is_some()
        || read.id.is_some()
        || read
            .connects
            .iter()
            .any(|c| c.data.args.is_some() || c.content.is_some() || c.data.value.is_some())
}

/// `:(args)` or `:{value}`: no name, so it is not a connect of its own but more
/// of the owner's args or value.
fn is_merge_connect(node: &SyntaxNode) -> bool {
    !node.children().any(|c| c.kind() == K::NAME)
}

/// Merges the groups of a nameless `:` connect into `read`, as the AST parser does.
fn merge_connect(node: &SyntaxNode, read: &mut Groups) -> Result<()> {
    for child in node.children() {
        match child.kind() {
            K::ARGS => {
                let args = crate::element::lower_paren_value(&child)?;
                read.data.args = merge_values(read.data.args.as_ref(), Some(&args));
            }
            K::VALUE_DATA | K::MAP_LITERAL => {
                let value = crate::element::lower_value_group(&child)?;
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
    end: usize,
    lines: &[SyntaxNode],
    lang: &str,
    body_start: usize,
) -> Result<Block> {
    let depth = pipe_depth(node);
    let mut code = String::new();
    for line in lines {
        let mut marker_count = 0;
        let mut line_content = String::new();
        let mut after_markers = depth == 0;
        let mut skipped_space = false;
        let mut has_newline = false;
        for elem in line.children_with_tokens() {
            if let NodeOrToken::Token(token) = elem {
                if !after_markers {
                    if token.kind() == K::MARGIN {
                        marker_count += 1;
                        if marker_count == depth {
                            after_markers = true;
                        }
                        continue;
                    } else if token.kind() == K::WHITESPACE {
                        continue;
                    }
                }
                if after_markers && !skipped_space {
                    skipped_space = true;
                    if token.kind() == K::WHITESPACE && token.text().starts_with(' ') {
                        let text = token.text();
                        if text.len() > 1 {
                            line_content.push_str(&text[1..]);
                        }
                        continue;
                    }
                }
                if token.kind() == K::NEWLINE {
                    has_newline = true;
                    continue;
                }
                line_content.push_str(token.text());
            }
        }
        if depth > 0 && marker_count < depth {
            return Err(unsupported("a fence line without the run's marker"));
        }
        code.push_str(&line_content);
        if has_newline {
            code.push('\n');
        }
    }
    if code.ends_with('\n') {
        code.pop();
    }
    let content_span = Span::new(body_start, end);
    let mut element = element_new(Name::bare("raw"))
        .with_placement(Placement::Block)
        .with_span(Span::new(start, end))
        .with_content(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Raw(RawText::new(code, content_span))],
            content_span,
        ))]);
    if !lang.is_empty() {
        element.data.args = Some(Value::Map(vec![(
            "lang".to_string(),
            Value::String(lang.to_string()),
        )]));
    }
    Ok(Block::Element(element))
}

fn token_column(tok: &SyntaxToken) -> usize {
    let mut col = 1;
    let mut prev = tok.prev_token();
    while let Some(t) = prev {
        if t.kind() == K::NEWLINE {
            break;
        }
        col += t.text().chars().count();
        prev = t.prev_token();
    }
    col
}

/// The error for a group of a kind the owner already has, written against the last one (the
/// CST keeps it in an `ERROR` node). `:` in front of it merges into the one that is there.
fn second_group(node: &SyntaxNode) -> Error {
    let (start, _) = range_of(node);
    let first = node.first_token().map(|t| t.kind());
    if first == Some(K::HASH) {
        return err(start, "a second `#(...)`: an element takes at most one id");
    }
    let group = match first {
        Some(K::L_PAREN) => "(args)",
        Some(K::L_BRACKET) | Some(K::PIPE) => "[content]",
        _ => "{value}",
    };
    err(
        start,
        format!(
            "a second `{group}` group: an element takes one of each. \
             Write `:` in front of it to merge into the one already there"
        ),
    )
}

pub(crate) fn lower_error(node: &SyntaxNode) -> Error {
    let start = usize::from(node.text_range().start());

    let first = node.first_token().map(|t| t.kind());
    if matches!(
        first,
        Some(K::HASH | K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE)
    ) && node.parent().is_some_and(|p| {
        matches!(
            p.kind(),
            K::BLOCK_ELEMENT | K::INLINE_ELEMENT | K::SECTION_HEADING | K::SLOT | K::CONNECT
        )
    }) {
        return second_group(node);
    }

    if first == Some(K::COMMENT) {
        return err(start, "unterminated block comment '/*', expected '*/'");
    }

    if node.parent().is_some_and(|p| p.kind() == K::THEMATIC_BREAK) {
        if node.children_with_tokens().count() == 0 {
            return err(
                start,
                "expected 3 or more '-' to close the titled thematic break",
            );
        } else {
            return err(
                start,
                "unexpected trailing content after titled thematic break",
            );
        }
    }

    if let Some(parent) = node.parent().filter(|p| p.kind() == K::CONTENT) {
        if parent.first_token().is_some_and(|t| t.kind() == K::L_BRACKET) {
            let eof = usize::from(node.ancestors().last().unwrap().text_range().end());
            return err(eof, "unterminated, expected ']'");
        }
        if parent.first_token().is_some_and(|t| t.kind() == K::PIPE) {
            let opener = |n: &SyntaxNode| {
                n.kind() == K::CONTENT && n.first_token().is_some_and(|t| t.kind() == K::PIPE)
            };
            let columns: Vec<usize> = parent
                .ancestors()
                .filter(opener)
                .filter_map(|n| n.first_token())
                .map(|t| token_column(&t))
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();

            let root = node.ancestors().last().unwrap();
            let end = usize::from(node.text_range().end());
            let tok = root
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| usize::from(t.text_range().start()) >= end)
                .find(|t| !matches!(t.kind(), K::WHITESPACE | K::NEWLINE));

            if let Some(mut look) = tok {
                for expected in columns {
                    if look.kind() != K::PIPE {
                        break;
                    }
                    let found = token_column(&look);
                    let pos = usize::from(look.text_range().start());
                    if found != expected {
                        return err(
                            pos,
                            format!(
                                "this `|` stands in column {found}, and the content it would \
                                 continue opens in column {expected}: it lines up with no content"
                            ),
                        );
                    }
                    let look_end = usize::from(look.text_range().end());
                    look = match root
                        .descendants_with_tokens()
                        .filter_map(|e| e.into_token())
                        .filter(|t| usize::from(t.text_range().start()) >= look_end)
                        .find(|t| t.kind() != K::WHITESPACE)
                    {
                        Some(n) => n,
                        None => break,
                    };
                }
            }
        }
    }

    err(start, "a `::` declaration stands alone on its line")
}

/// The box an element node holds, if it is one: `<?matcher...>` is an element of
/// the tree, where a name tag (`<name>`) is a `SLOT` among the text and never wrapped.
pub(crate) fn slot_of(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children().find(|n| n.kind() == K::SLOT)
}

/// `<?matcher(args){value}>[content]`: a hole, from the `SLOT` node of a marked box.
pub(crate) fn lower_slot(slot: &SyntaxNode) -> Result<tomet_ast::Hole> {
    let (start, end) = range_of(slot);
    let matcher = slot
        .children()
        .find(|n| n.kind() == K::NAME)
        .and_then(CstName::cast)
        .map(|c| {
            let (namespace, name) = c.parts();
            Name { namespace, name }
        });
    let read = lower_groups(slot)?;
    Ok(tomet_ast::Hole {
        matcher,
        placement: Placement::Inline,
        data: read.data,
        content: read.content,
        span: Span::new(start, end),
    })
}

/// `connected` merged under `direct`: maps key by key (the direct value wins a
/// clash), sequences without repeating an item, anything else the direct one.
pub(crate) fn merge_values(direct: Option<&Value>, connected: Option<&Value>) -> Option<Value> {
    match (direct, connected) {
        (None, None) => None,
        (Some(d), None) => Some(d.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(d), Some(c)) => Some(merge_values_inner(d, c)),
    }
}

fn merge_values_inner(direct: &Value, connected: &Value) -> Value {
    match (direct, connected) {
        (Value::Map(d_map), Value::Map(c_map)) => {
            let mut merged = c_map.clone();
            for (d_k, d_v) in d_map {
                if let Some(pos) = merged.iter().position(|(c_k, _)| c_k == d_k) {
                    let c_v = &merged[pos].1;
                    merged[pos].1 = merge_values_inner(d_v, c_v);
                } else {
                    merged.push((d_k.clone(), d_v.clone()));
                }
            }
            Value::Map(merged)
        }
        (Value::Seq(d_seq), Value::Seq(c_seq)) => {
            let mut merged = c_seq.clone();
            for item in d_seq {
                if !merged.contains(item) {
                    merged.push(item.clone());
                }
            }
            Value::Seq(merged)
        }
        (direct_val, _) => direct_val.clone(),
    }
}

pub(crate) fn trim_trailing_equals(inlines: &mut Vec<Inline>) {
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
