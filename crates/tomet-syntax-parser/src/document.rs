//! Document-level parsing for `.tmt` / `.tmt` files.
//!
//! Orchestrates block-level constructs (headings, paragraphs, lists,
//! elements, thematic breaks) into a [`Document`].

use crate::codeblock::{is_fenced_code_block_start, parse_fenced_code_block};
use crate::element::{
    LineEnd, consume_trailing_continuation, element_ends_line, is_element_start,
    leading_continuation, parse_element,
};
use crate::error::Result;
use crate::heading::{
    consume_thematic_break, is_thematic_break, is_titled_thematic_break_start, parse_heading,
    parse_titled_thematic_break,
};
use crate::inline::{Stop, extend_merging, parse_inline_seq, push_soft_break};
use crate::interp::{is_interp_start, parse_dollar_element};
use crate::list::{parse_list, peek_list_marker};
use crate::value::{skip_inline_ws, skip_ws_and_newlines};
use tomet_ast::{Block, Document, Element, Inline, Paragraph, Placement};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_list, element_new};

/// Parse an entire source string as a markup [`Document`].
pub fn parse_document(src: &str) -> Result<Document> {
    let mut cur = Cursor::new(src);
    let start_pos = cur.pos();
    let mut blocks = Vec::new();

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
        // its kind (heading, list, a bare `@name`, ...) -- the parser
        // stays kind-oblivious; `tomet-semantics::shape_mismatch` is what
        // flags a kind that cannot actually be inline. Dangling (nothing
        // valid to pop) is a silent no-op: the parser keeps no trace of a
        // swallowed trigger.
        if let Some(after) = leading_continuation(&cur) {
            cur = after;
            if matches!(blocks.last(), Some(Block::Element(_))) {
                let Some(Block::Element(prev)) = blocks.pop() else {
                    unreachable!()
                };
                blocks.push(continue_into_paragraph(&mut cur, prev)?);
            }
            continue;
        }

        if cur.peek() == Some('#') && is_heading_start(&cur) {
            let el = parse_heading(&mut cur)?;
            blocks.push(Block::Element(el));
            continue;
        }
        if is_titled_thematic_break_start(&cur) {
            let el = parse_titled_thematic_break(&mut cur)?;
            blocks.push(Block::Element(el));
            continue;
        }
        if is_thematic_break(&cur) {
            let item_start = cur.pos();
            consume_thematic_break(&mut cur);
            let el = element_new(tomet_ast::Sigil::named("hr"))
                .with_placement(Placement::Block)
                .with_span(cur.span_from(item_start));
            blocks.push(Block::Element(el));
            continue;
        }
        if is_fenced_code_block_start(&cur) {
            let el = parse_fenced_code_block(&mut cur)?;
            blocks.push(Block::Element(el));
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
                blocks.push(Block::Element(el));
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
                    blocks.push(Block::Element(el));
                    continue;
                }
                LineEnd::Continuation => {
                    let el = parse_element(&mut cur, true)?;
                    consume_trailing_continuation(&mut cur);
                    consume_bare_line_end(&mut cur);
                    blocks.push(continue_into_paragraph(&mut cur, el)?);
                    continue;
                }
                LineEnd::No => {}
            }
        }
        if cur.peek() == Some('$') && is_interp_start(&cur) {
            let el = parse_dollar_element(&mut cur)?;
            blocks.push(Block::Element(el));
            continue;
        }

        blocks.push(parse_paragraph(&mut cur)?);
    }

    let span = cur.span_from(start_pos);
    Ok(Document::new(blocks, span))
}

/// Consumes the inline whitespace/comments and the line's own trailing
/// newline after a bare element already confirmed [`LineEnd::Bare`] or
/// [`LineEnd::Continuation`] (and, for the latter, after the trigger
/// itself has already been consumed) -- the same gap `element_ends_line`'s
/// probe already walked through without consuming. Leaves `cur` at the
/// start of the next line.
fn consume_bare_line_end(cur: &mut Cursor) {
    // A `+++` fence swallows its own closing line, newline included
    // (`element.rs::element_ends_line`'s probe accounts for the same
    // case), so `cur` can already sit at the start of the next line with
    // nothing of this line left to skip. Bumping a newline anyway would
    // eat that next line's own leading newline -- silently erasing a
    // blank-line separator when the next line happens to be blank, which
    // is exactly the join/isolate decision this function must not affect.
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
///
/// The parser is kind-oblivious about what it just joined (`first` may be
/// a heading, a list, a fenced code block, or an ordinary `@name`
/// element) -- `tomet-semantics::shape_mismatch` is what reports it back
/// if the kind cannot actually be `Inline`.
fn continue_into_paragraph(cur: &mut Cursor, first: Element) -> Result<Block> {
    let para_start = first.span.start.offset;
    let break_start = first.span.end.offset;
    // A redundant leading `\` can sit right here, at the very position
    // `parse_inline_seq` is about to start scanning from: both callers
    // (the leading-trigger branch above, and `LineEnd::Continuation`'s
    // trailing-trigger branch) already consumed their own newline before
    // reaching this point, so `Stop::Paragraph`'s own "peek past an
    // in-scan newline" redundant-marker check never gets a chance to see
    // this one -- it is not past a newline the scan itself crossed, it is
    // sitting at the scan's own start. Consume it here instead, the same
    // way it would be folded away mid-run.
    if let Some(after) = leading_continuation(cur) {
        *cur = after;
    }
    let join_span = cur.span_from(break_start);
    let mut content = vec![Inline::Element(first.with_placement(Placement::Inline))];
    push_soft_break(&mut content, join_span);
    let rest = parse_inline_seq(cur, Stop::Paragraph, true)?;
    extend_merging(&mut content, rest);
    // `rest` was already trimmed of its own trailing `SoftBreak` by
    // `parse_inline_seq`, but if `rest` was empty (nothing followed after
    // all -- e.g. a trigger right before a heading, which
    // `Stop::Paragraph` does not know how to absorb as content) the join
    // break above is left dangling at the very end; drop it the same way.
    if matches!(content.last(), Some(Inline::SoftBreak(_))) {
        content.pop();
    }
    let span = cur.span_from(para_start);
    Ok(Block::Paragraph(Paragraph::new(content, span)))
}

/// A run of `#` followed by `[` is a heading -- `#[ Title ]`, `##[ ... ]`
/// for level 2, and so on.
///
/// `#` introduces nothing else. It briefly doubled as the block-element
/// sigil, which forced a lookahead past the `#` run to tell `#name` from
/// `#[`; elements are spelled `@name` regardless of placement now, so the
/// glyph is the heading marker and only that.
pub(crate) fn is_heading_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.eat_while(|c| c == '#');
    // A group may follow the run directly: `#[ x ]`, `#(id: a)[ x ]`,
    // `#| x` -- the last being `[content]` without the brackets.
    if crate::element::opens_group(look.peek()) {
        return true;
    }
    // Otherwise the bracket-less sugar, which needs a space to separate
    // the run from its content. That space is what keeps `#tag` prose.
    if !matches!(look.peek(), Some(' ') | Some('\t')) {
        return false;
    }
    skip_inline_ws(&mut look);
    !matches!(look.peek(), None | Some('\n') | Some('\r'))
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
///
/// A paragraph whose whole content is a single element used to be promoted
/// to `Block::Element` here. The placement rule makes that promotion both
/// unnecessary and wrong: an `@name` element that stands alone on its line
/// is already taken by the block branch above with `Placement::Block`, and
/// what still reaches this point is markup that only ever exists inline --
/// a line holding nothing but `*strong*` is a paragraph, not a block-placed
/// `strong` that `shape_mismatch` would then reject.
fn parse_paragraph(cur: &mut Cursor) -> Result<Block> {
    let start_pos = cur.pos();
    let content = parse_inline_seq(cur, Stop::Paragraph, true)?;
    let span = cur.span_from(start_pos);
    Ok(Block::Paragraph(Paragraph::new(content, span)))
}
