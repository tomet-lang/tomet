//! Document-level parsing for `.tmt` / `.tmt` files.
//!
//! Orchestrates block-level constructs (sections, paragraphs, lists,
//! elements, thematic breaks) into a [`Document`].

use crate::codeblock::{is_fenced_code_block_start, parse_fenced_code_block};
use crate::element::{
    LineEnd, consume_trailing_continuation, element_ends_line, is_element_start,
    leading_continuation, parse_element,
};
use crate::error::Result;
use crate::inline::{Stop, extend_merging, parse_inline_seq, push_soft_break};
use crate::interp::{is_interp_start, parse_dollar_element};
use crate::list::{parse_list, peek_list_marker};
use crate::section::{
    consume_thematic_break, is_section_start, is_thematic_break, is_titled_thematic_break_start,
    parse_section, parse_titled_thematic_break,
};
use crate::value::{skip_inline_ws, skip_ws_and_newlines};
use tomet_ast::{Block, Document, Element, Inline, Paragraph, Placement, Section};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_list, element_new};

/// Parse an entire source string as a markup [`Document`].
pub fn parse_document(src: &str) -> Result<Document> {
    let mut cur = Cursor::new(src);
    let start_pos = cur.pos();
    let mut doc_blocks: Vec<Block> = Vec::new();
    let mut stack: Vec<Section> = Vec::new();

    loop {
        skip_ws_and_newlines(&mut cur);
        if is_line_comment_start(&cur) {
            skip_line_comment(&mut cur);
            continue;
        }
        if is_block_comment_start(&cur) {
            crate::value::skip_block_comment(&mut cur)?;
            continue;
        }
        if cur.is_eof() {
            break;
        }

        // A leading `\` continuation trigger (`docs/spec/syntax.tmt`'s
        // `##[ 継続 ]`) joins whatever follows to the last block, whatever
        // its kind (section, list, a bare `@name`, ...) -- the parser
        // stays kind-oblivious; `tomet-semantics::shape_mismatch` is what
        // flags a kind that cannot actually be inline. Dangling (nothing
        // valid to pop) is a silent no-op: the parser keeps no trace of a
        // swallowed trigger.
        if let Some(after) = leading_continuation(&cur) {
            cur = after;
            let last_b = if let Some(top) = stack.last_mut() {
                top.blocks.last()
            } else {
                doc_blocks.last()
            };
            if matches!(last_b, Some(Block::Element(_))) {
                let prev = if let Some(top) = stack.last_mut() {
                    let Some(Block::Element(el)) = top.blocks.pop() else {
                        unreachable!()
                    };
                    el
                } else {
                    let Some(Block::Element(el)) = doc_blocks.pop() else {
                        unreachable!()
                    };
                    el
                };
                let cont = continue_into_paragraph(&mut cur, prev)?;
                if let Some(top) = stack.last_mut() {
                    top.blocks.push(cont);
                } else {
                    doc_blocks.push(cont);
                }
            }
            continue;
        }

        if cur.peek() == Some('=') && is_section_start(&cur) {
            let sec = parse_section(&mut cur)?;
            push_section(&mut doc_blocks, &mut stack, sec);
            continue;
        }
        if is_titled_thematic_break_start(&cur) {
            let el = parse_titled_thematic_break(&mut cur)?;
            push_block(&mut doc_blocks, &mut stack, Block::Element(el));
            continue;
        }
        if is_thematic_break(&cur) {
            let item_start = cur.pos();
            consume_thematic_break(&mut cur);
            let el = element_new(tomet_ast::Sigil::named("hr"))
                .with_placement(Placement::Block)
                .with_span(cur.span_from(item_start));
            push_block(&mut doc_blocks, &mut stack, Block::Element(el));
            continue;
        }
        if is_fenced_code_block_start(&cur) {
            let el = parse_fenced_code_block(&mut cur)?;
            push_block(&mut doc_blocks, &mut stack, Block::Element(el));
            continue;
        }
        if let Some(head) = peek_list_marker(&cur)? {
            let items = parse_list(&mut cur, head.ordered)?;
            if !items.is_empty() {
                let list_span = items
                    .first()
                    .unwrap()
                    .span
                    .union(&items.last().unwrap().span);
                let el = element_list(head.ordered, items, list_span);
                push_block(&mut doc_blocks, &mut stack, Block::Element(el));
            }
            continue;
        }
        // Join/isolate rule (`docs/spec/syntax.tmt`'s `##[ 継続 ]`): a bare
        // element that opens its own line isolates as its own block by
        // default -- `docs/examples/dirs.tmt`'s file listing depends on
        // this -- and joins whatever follows only when its line ends with
        // an explicit trailing `\`.
        if cur.peek() == Some('@') && is_element_start(&cur, true) {
            match element_ends_line(&cur) {
                LineEnd::Bare => {
                    let el = parse_element(&mut cur, true)?.with_placement(Placement::Block);
                    consume_bare_line_end(&mut cur);
                    push_block(&mut doc_blocks, &mut stack, Block::Element(el));
                    continue;
                }
                LineEnd::Continuation => {
                    let el = parse_element(&mut cur, true)?;
                    consume_trailing_continuation(&mut cur);
                    consume_bare_line_end(&mut cur);
                    let cont = continue_into_paragraph(&mut cur, el)?;
                    push_block(&mut doc_blocks, &mut stack, cont);
                    continue;
                }
                LineEnd::No => {}
            }
        }
        if cur.peek() == Some('$') && is_interp_start(&cur) {
            let el = parse_dollar_element(&mut cur)?;
            push_block(&mut doc_blocks, &mut stack, Block::Element(el));
            continue;
        }

        let p = parse_paragraph(&mut cur)?;
        push_block(&mut doc_blocks, &mut stack, p);
    }

    // Flush all remaining open sections from innermost to outermost
    while let Some(finished) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.blocks.push(Block::Section(finished));
        } else {
            doc_blocks.push(Block::Section(finished));
        }
    }

    let span = cur.span_from(start_pos);
    Ok(Document::new(doc_blocks, span))
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

/// Consumes the inline whitespace/comments and the line's own trailing
/// newline after a bare element already confirmed [`LineEnd::Bare`] or
/// [`LineEnd::Continuation`] (and, for the latter, after the trigger
/// itself has already been consumed) -- the same gap `element_ends_line`'s
/// probe already walked through without consuming. Leaves `cur` at the
/// start of the next line.
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
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }
}

/// Builds a paragraph that starts from an already-parsed `first` element
/// (demoted to `Placement::Inline`, whatever it was) instead of from raw
/// source, joined to whatever follows by one `SoftBreak` spanning
/// `first`'s own end to wherever the rest resumes. `cur` must already sit
/// at the start of that following content.
fn continue_into_paragraph(cur: &mut Cursor, first: Element) -> Result<Block> {
    let para_start = first.span.start.offset;
    let break_start = first.span.end.offset;
    if let Some(after) = leading_continuation(cur) {
        *cur = after;
    }
    let join_span = cur.span_from(break_start);
    let mut content = vec![Inline::Element(first.with_placement(Placement::Inline))];
    push_soft_break(&mut content, join_span);
    let rest = parse_inline_seq(cur, Stop::Paragraph, true)?;
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

fn skip_line_comment(cur: &mut Cursor) {
    skip_inline_ws(cur);
    cur.eat_str("//");
    cur.eat_while(|c| c != '\n' && c != '\r');
}

/// Parses a paragraph.
fn parse_paragraph(cur: &mut Cursor) -> Result<Block> {
    let start_pos = cur.pos();
    let content = parse_inline_seq(cur, Stop::Paragraph, true)?;
    let span = cur.span_from(start_pos);
    Ok(Block::Paragraph(Paragraph::new(content, span)))
}
