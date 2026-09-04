//! Document-level parsing for `.tmt` / `.tmt` files.
//!
//! Orchestrates block-level constructs (headings, paragraphs, lists,
//! elements, thematic breaks) into a [`Document`].

use crate::codeblock::{is_fenced_code_block_start, parse_fenced_code_block};
use crate::element::{element_ends_line, is_element_start, parse_element};
use crate::error::Result;
use crate::heading::{
    consume_thematic_break, is_thematic_break, is_titled_thematic_break_start, parse_heading,
    parse_titled_thematic_break,
};
use crate::inline::{Stop, parse_inline_seq};
use crate::interp::{is_interp_start, parse_dollar_element};
use crate::list::{parse_list, peek_list_marker};
use crate::value::{skip_inline_ws, skip_ws_and_newlines};
use tomet_ast::{Block, Document, Paragraph, Placement};
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

        if cur.peek() == Some('#') && is_heading_start(&cur) {
            blocks.push(Block::Element(parse_heading(&mut cur)?));
            continue;
        }
        if is_titled_thematic_break_start(&cur) {
            blocks.push(Block::Element(parse_titled_thematic_break(&mut cur)?));
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
            blocks.push(Block::Element(parse_fenced_code_block(&mut cur)?));
            continue;
        }
        if let Some((ordered, ..)) = peek_list_marker(&cur)? {
            let items = parse_list(&mut cur, ordered)?;
            if !items.is_empty() {
                let list_span = items
                    .first()
                    .unwrap()
                    .span
                    .union(&items.last().unwrap().span);
                blocks.push(Block::Element(element_list(ordered, items, list_span)));
            }
            continue;
        }
        // Placement rule, second half: the cursor is in block context here,
        // so an element becomes a block only when it also ends its line.
        // One that has more content after it opens a paragraph instead --
        // `@link(…)[Tomet] は軽量マークアップ言語です。` is one paragraph,
        // not an element with an orphaned sentence behind it.
        if cur.peek() == Some('@') && is_element_start(&cur, true) && element_ends_line(&cur) {
            let el = parse_element(&mut cur, true)?.with_placement(Placement::Block);
            blocks.push(Block::Element(el));
            continue;
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
    // A group may follow the run directly: `#[ x ]`, `#(id: a)[ x ]`.
    if matches!(look.peek(), Some('[') | Some('(') | Some('{')) {
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
