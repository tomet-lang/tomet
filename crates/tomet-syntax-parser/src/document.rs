//! Document-level parsing for `.tmt` / `.tmt` files.
//!
//! Orchestrates block-level constructs (headings, paragraphs, lists,
//! elements, thematic breaks) into a [`Document`].

use crate::codeblock::{is_fenced_code_block_start, parse_fenced_code_block};
use crate::element::{is_block_element_start, is_inline_element_start, parse_element};
use crate::error::Result;
use crate::heading::{
    consume_thematic_break, is_thematic_break, is_titled_thematic_break_start, parse_heading,
    parse_titled_thematic_break,
};
use crate::inline::{Stop, parse_inline_seq};
use crate::interp::{is_interp_start, parse_dollar_element};
use crate::list::{parse_list, peek_list_marker};
use crate::value::{skip_inline_ws, skip_ws_and_newlines};
use tomet_ast::{Block, Document, Inline, Paragraph};
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
        if cur.peek() == Some('#') && is_block_element_start(&cur) {
            blocks.push(Block::Element(parse_element(&mut cur, true)?));
            continue;
        }
        if is_titled_thematic_break_start(&cur) {
            blocks.push(Block::Element(parse_titled_thematic_break(&mut cur)?));
            continue;
        }
        if is_thematic_break(&cur) {
            let item_start = cur.pos();
            consume_thematic_break(&mut cur);
            let el =
                element_new(tomet_ast::Sigil::block("hr")).with_span(cur.span_from(item_start));
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
        if cur.peek() == Some('@') && is_inline_element_start(&cur) {
            blocks.push(Block::Element(parse_element(&mut cur, true)?));
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
/// This is checked before [`is_block_element_start`], and the two are
/// disambiguated by one character of lookahead after the `#` run: `[`
/// (optionally preceded by inline whitespace) means heading, an identifier
/// immediately adjacent means block element. A run longer than one `#`
/// requires `[` -- `##name` is not an element.
pub(crate) fn is_heading_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.eat_while(|c| c == '#');
    skip_inline_ws(&mut look);
    look.peek() == Some('[')
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

fn parse_paragraph(cur: &mut Cursor) -> Result<Block> {
    let start_pos = cur.pos();
    let mut content = parse_inline_seq(cur, Stop::Paragraph, true)?;
    let span = cur.span_from(start_pos);
    if content.len() == 1 && matches!(content[0], Inline::Element(_)) {
        if let Inline::Element(mut el) = content.pop().unwrap() {
            if el.span == tomet_ast::Span::default() {
                el.span = span;
            }
            return Ok(Block::Element(el));
        }
    }
    Ok(Block::Paragraph(Paragraph::new(content, span)))
}
