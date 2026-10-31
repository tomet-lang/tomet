//! Document-level parsing for `.tmt` files.
//!
//! Orchestrates block-level constructs (sections, paragraphs, lists,
//! elements, thematic breaks) into a [`Document`].
//!
//! A blank line ends a paragraph, and so does a `;` at the start of a line: it
//! starts the next block without one. The `;` leaves nothing in the tree.

use crate::dialect::Dialect;
use crate::element::{
    Written, element_stands_alone, is_document_element_start, is_element_start, parse_declaration,
    parse_written,
};
use crate::error::Result;
use crate::inline::{
    PipeContinuation, Stop, at_marker_line_start, extend_merging, paragraph_breaks_here,
    parse_inline_seq, pipe_run_state, push_soft_break,
};
use crate::interp::{is_interp_start, parse_interpolation};
use crate::list::{parse_list, peek_list_marker};
use crate::raw::{is_fenced_code_block_start, parse_fenced_code_block};
use crate::section::{
    consume_thematic_break, is_section_start, is_thematic_break, is_titled_thematic_break_start,
    parse_section, parse_titled_thematic_break,
};
use crate::value::{skip_inline_ws, skip_ws_and_newlines};
use tomet_ast::{Block, Document, Inline, Paragraph, Placement, Section};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new, list};

/// Parse an entire source string as a markup [`Document`].
pub fn parse_document(src: &str) -> Result<Document> {
    parse_in(Dialect::Document, src)
}

/// Parse an entire source string as a pattern: a [`Document`] in which `?name(args){options}[content]`
/// is a hole ([`tomet_ast::Hole`]).
///
/// It is [`parse_document`] with one more sigil. Everything else is read by the same code, so a
/// pattern is written in, and printed as, ordinary tomet.
pub fn parse_pattern(src: &str) -> Result<Document> {
    parse_in(Dialect::Pattern, src)
}

fn parse_in(dialect: Dialect, src: &str) -> Result<Document> {
    let mut cur = Cursor::new(src);
    let start_pos = cur.pos();
    let blocks =
        parse_block_seq(dialect, &mut cur, BlockStop::Eof, &[]).map_err(|e| e.located_in(src))?;
    let span = cur.span_from(start_pos);
    Ok(Document::new(blocks, span))
}

/// What ends a [`parse_block_seq`] run.
///
/// `Eof` is the top level (a whole document): nothing closes it but running
/// out of source. `Bracket` is `[content]`'s own closing character (always
/// `']'` today): the block sequence stops right before it, unconsumed, for
/// the caller (`element::parse_content`) to eat, and running out of source
/// first is an error, not a silent stop -- same asymmetry [`Stop::Bracket`]
/// already has at the inline level.
#[derive(Debug, Clone, Copy)]
pub(crate) enum BlockStop {
    Eof,
    Bracket(char),
}

/// The block-sequence grammar shared by a whole document, `[content]`, and
/// `|content`: sections, lists, elements, fenced code, thematic breaks and
/// paragraphs, orchestrated exactly the same way regardless of what bounds
/// the sequence. A bounded call (`BlockStop::Bracket` or active `pipe_stack`)
/// gets its own local `stack` -- a section opened inside `[content]` or
/// `|content` is fully closed by the time this returns, independent of
/// whatever section the *enclosing* document is in the middle of.
pub(crate) fn parse_block_seq(
    dialect: Dialect,
    cur_ref: &mut Cursor,
    stop: BlockStop,
    pipe_stack: &[usize],
) -> Result<Vec<Block>> {
    let mut cur = *cur_ref;
    let mut doc_blocks: Vec<Block> = Vec::new();
    let mut stack: Vec<Section> = Vec::new();
    let boundary = match stop {
        BlockStop::Eof => None,
        BlockStop::Bracket(c) => Some(c),
    };

    loop {
        if pipe_stack.is_empty() {
            skip_ws_and_newlines(&mut cur);
        } else {
            skip_inline_ws(&mut cur);
            let mut ended = false;
            while matches!(cur.peek(), Some('\n') | Some('\r')) {
                match crate::inline::pipe_run_state(&cur, pipe_stack)? {
                    crate::inline::PipeContinuation::End => {
                        ended = true;
                        break;
                    }
                    crate::inline::PipeContinuation::Empty => {
                        cur.bump();
                        for _ in 0..pipe_stack.len() {
                            skip_inline_ws(&mut cur);
                            cur.bump();
                        }
                        skip_inline_ws(&mut cur);
                    }
                    crate::inline::PipeContinuation::Content => {
                        cur.bump();
                        for _ in 0..pipe_stack.len() {
                            skip_inline_ws(&mut cur);
                            cur.bump();
                        }
                        skip_inline_ws(&mut cur);
                        break;
                    }
                }
            }
            if ended {
                break;
            }
        }

        if is_line_comment_start(&cur) {
            skip_line_comment(&mut cur);
            continue;
        }
        if is_block_comment_start(&cur) {
            crate::value::skip_block_comment(&mut cur)?;
            continue;
        }
        match stop {
            BlockStop::Eof => {
                if cur.is_eof() {
                    break;
                }
            }
            BlockStop::Bracket(c) => {
                if cur.is_eof() {
                    *cur_ref = cur;
                    return Err(crate::value::err(
                        cur.pos(),
                        format!("unterminated, expected '{c}'"),
                    ));
                }
                if cur.peek() == Some(c) {
                    break;
                }
            }
        }

        // A `;` at the start of a line breaks the block: what follows starts a
        // new one, as it would after a blank line, with none needed. The
        // marker itself leaves nothing in the tree.
        if cur.peek() == Some(';') && at_marker_line_start(&cur) {
            cur.bump();
            skip_inline_ws(&mut cur);
            continue;
        }

        if cur.peek() == Some('=') && is_section_start(&cur) {
            let sec = parse_section(dialect, &mut cur)?;
            push_section(&mut doc_blocks, &mut stack, sec);
            continue;
        }
        if is_titled_thematic_break_start(&cur) {
            let el = parse_titled_thematic_break(dialect, &mut cur)?;
            push_block(&mut doc_blocks, &mut stack, Block::Element(el));
            continue;
        }
        if is_thematic_break(&cur) {
            let item_start = cur.pos();
            consume_thematic_break(&mut cur);
            let el = element_new(tomet_ast::Name::bare("hr"))
                .with_placement(Placement::Block)
                .with_span(cur.span_from(item_start));
            push_block(&mut doc_blocks, &mut stack, Block::Element(el));
            continue;
        }
        if is_fenced_code_block_start(&cur) {
            let el = parse_fenced_code_block(&mut cur, pipe_stack)?;
            push_block(&mut doc_blocks, &mut stack, Block::Element(el));
            continue;
        }
        if let Some(head) = peek_list_marker(dialect, &cur)? {
            let items = parse_list(dialect, &mut cur, head.ordered, pipe_stack)?;
            if !items.is_empty() {
                let list_span = items
                    .first()
                    .unwrap()
                    .span
                    .union(&items.last().unwrap().span);
                let l = list(head.ordered, items, list_span);
                push_block(&mut doc_blocks, &mut stack, Block::List(l));
            }
            continue;
        }
        // A declaration is only ever a block, so the join/isolate rule has
        // nothing to decide for it. It is not allowed to fall back to
        // running text either: `::kind(a, ` is a broken declaration, not a
        // paragraph that happens to start with two colons.
        if is_document_element_start(&cur) {
            let declaration = parse_declaration(dialect, &mut cur, pipe_stack)?;
            let mut rest = cur;
            skip_inline_ws(&mut rest);
            if !matches!(rest.peek(), None | Some('\n') | Some('\r'))
                && !rest.starts_with("//")
                && !rest.starts_with("/*")
            {
                return Err(crate::value::err(
                    rest.pos(),
                    "a `::` declaration stands alone on its line",
                ));
            }
            consume_bare_line_end(&mut cur);
            push_block(&mut doc_blocks, &mut stack, Block::Declaration(declaration));
            continue;
        }
        // An element that is the whole of its line is a block of its own,
        // unless the next line folds into it: running text goes on across
        // lines, and an element is running text like any other, so
        // `@a()` then `@b()` on consecutive lines are one paragraph. A `;`
        // or a blank line is what makes them blocks.
        if dialect.opens_element(cur.peek())
            && is_element_start(dialect, &cur, true)
            && element_stands_alone(dialect, &cur)
        {
            let written = parse_written(dialect, &mut cur, true, pipe_stack)?;
            consume_bare_line_end(&mut cur);
            if next_line_folds(dialect, &cur, boundary, pipe_stack)? {
                let cont = continue_into_paragraph(dialect, &mut cur, written, boundary, pipe_stack)?;
                push_block(&mut doc_blocks, &mut stack, cont);
            } else {
                let block = written.with_placement(Placement::Block).into_block();
                push_block(&mut doc_blocks, &mut stack, block);
            }
            continue;
        }
        // The same for a `${...}` that is the whole of its line; anything
        // else around it makes it running text.
        if cur.peek() == Some('$') && is_interp_start(&cur) {
            let mut probe = cur;
            let interp = parse_interpolation(&mut probe)?;
            let mut rest = probe;
            skip_inline_ws(&mut rest);
            let alone = matches!(rest.peek(), None | Some('\n') | Some('\r'))
                || rest.starts_with("//")
                || rest.starts_with("/*");
            if alone {
                consume_bare_line_end(&mut probe);
                if !next_line_folds(dialect, &probe, boundary, pipe_stack)? {
                    cur = probe;
                    push_block(&mut doc_blocks, &mut stack, Block::Interp(interp));
                    continue;
                }
            }
        }

        if let Some(p) = parse_paragraph(dialect, &mut cur, boundary, pipe_stack)? {
            push_block(&mut doc_blocks, &mut stack, p);
        }
    }

    // Flush all remaining open sections from innermost to outermost
    while let Some(finished) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.blocks.push(Block::Section(finished));
        } else {
            doc_blocks.push(Block::Section(finished));
        }
    }

    *cur_ref = cur;
    Ok(doc_blocks)
}

fn push_block(doc_blocks: &mut Vec<Block>, stack: &mut [Section], block: Block) {
    if let Some(top) = stack.last_mut() {
        top.blocks.push(block);
    } else {
        doc_blocks.push(block);
    }
}

fn push_section(doc_blocks: &mut Vec<Block>, stack: &mut Vec<Section>, sec: Section) {
    while let Some(top) = stack.last() {
        if top.level >= sec.level {
            let finished = stack.pop().unwrap();
            if let Some(parent) = stack.last_mut() {
                parent.blocks.push(Block::Section(finished));
            } else {
                doc_blocks.push(Block::Section(finished));
            }
        } else {
            break;
        }
    }
    stack.push(sec);
}

fn consume_bare_line_end(cur: &mut Cursor) {
    if cur.pos() == 0 || cur.src()[..cur.pos()].ends_with(['\n', '\r']) {
        return;
    }
    skip_inline_ws(cur);
    loop {
        if cur.starts_with("//") {
            skip_line_comment(cur);
        } else if cur.starts_with("/*") {
            if crate::value::skip_block_comment(cur).is_err() {
                return;
            }
        } else {
            break;
        }
        skip_inline_ws(cur);
    }
}

/// Whether the line after the one `cur` ends (it stands at the line end)
/// folds into the paragraph that line opens: it is there, it is part of the same
/// bracket or `|` run, and it is not something that breaks a paragraph (a blank
/// line, a `;`, a heading, a list marker, ...). A `::` declaration is not
/// folded either: it is only ever a block.
fn next_line_folds(
    dialect: Dialect,
    cur: &Cursor,
    boundary: Option<char>,
    pipe_stack: &[usize],
) -> Result<bool> {
    if !matches!(cur.peek(), Some('\n') | Some('\r')) {
        return Ok(false);
    }
    let mut look = *cur;
    if pipe_stack.is_empty() {
        look.bump();
        skip_inline_ws(&mut look);
    } else {
        if !matches!(pipe_run_state(&look, pipe_stack)?, PipeContinuation::Content) {
            return Ok(false);
        }
        look.bump();
        for _ in 0..pipe_stack.len() {
            skip_inline_ws(&mut look);
            look.bump();
        }
        skip_inline_ws(&mut look);
    }
    if boundary.is_some_and(|c| look.peek() == Some(c)) {
        return Ok(false);
    }
    Ok(!paragraph_breaks_here(dialect, &look) && !is_document_element_start(&look))
}

/// Opens a paragraph with `first`, an element that was the whole of its line,
/// and reads the lines that fold into it. `cur` stands at the end of that line.
fn continue_into_paragraph(
    dialect: Dialect,
    cur: &mut Cursor,
    first: Written,
    boundary: Option<char>,
    pipe_stack: &[usize],
) -> Result<Block> {
    let para_start = first.span().start;
    let break_start = first.span().end;
    // Past the line end, and the `|` markers of the next line in a run.
    cur.bump();
    for _ in 0..pipe_stack.len() {
        skip_inline_ws(cur);
        cur.bump();
    }
    skip_inline_ws(cur);
    let join_span = cur.span_from(break_start);
    let mut content = vec![first.with_placement(Placement::Inline).into_inline()];
    push_soft_break(&mut content, join_span);
    let stop = if pipe_stack.is_empty() {
        Stop::Paragraph { boundary }
    } else {
        Stop::PipeRun { cols: pipe_stack }
    };
    let rest = parse_inline_seq(dialect, cur, stop, true)?;
    extend_merging(&mut content, rest);
    if matches!(content.last(), Some(Inline::SoftBreak(_))) {
        content.pop();
    }
    let span = cur.span_from(para_start);
    Ok(Block::Paragraph(Paragraph::new(content, span)))
}

fn is_line_comment_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    skip_inline_ws(&mut look);
    look.starts_with("//")
}

fn is_block_comment_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    skip_inline_ws(&mut look);
    look.starts_with("/*")
}

pub(crate) fn skip_line_comment(cur: &mut Cursor) {
    skip_inline_ws(cur);
    cur.eat_str("//");
    cur.eat_while(|c| c != '\n' && c != '\r');
}

fn parse_paragraph(
    dialect: Dialect,
    cur: &mut Cursor,
    boundary: Option<char>,
    pipe_stack: &[usize],
) -> Result<Option<Block>> {
    let start_pos = cur.pos();
    let content = if pipe_stack.is_empty() {
        parse_inline_seq(dialect, cur, Stop::Paragraph { boundary }, true)?
    } else {
        parse_inline_seq(dialect, cur, Stop::PipeRun { cols: pipe_stack }, true)?
    };
    if content.is_empty() {
        return Ok(None);
    }
    let span = cur.span_from(start_pos);
    Ok(Some(Block::Paragraph(Paragraph::new(content, span))))
}
