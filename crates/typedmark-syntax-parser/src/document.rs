//! Document-level parsing for `.tm` / `.tmt` files.
//!
//! Orchestrates block-level constructs (headings, paragraphs, lists,
//! elements, thematic breaks) into a [`Document`].

use crate::codeblock::{is_fenced_code_block_start, parse_fenced_code_block};
use crate::element::{
    config_format_update, is_at_element_start, is_type_element_start, parse_element,
};
use crate::embedded_format::EmbeddedFormat;
use crate::error::Result;
use crate::heading::{
    consume_thematic_break, is_thematic_break, is_titled_thematic_break_start, parse_heading,
    parse_titled_thematic_break,
};
use crate::inline::{Stop, parse_inline_seq};
use crate::interp::{is_interp_start, parse_dollar_element};
use crate::list::{parse_list, peek_list_marker};
use crate::value::{skip_inline_ws, skip_ws_and_newlines};
use typedmark_ast::{Block, Document, Inline, Paragraph};
use typedmark_lexar::Cursor;

/// Parse an entire source string as a markup [`Document`].
pub fn parse_document(src: &str) -> Result<Document> {
    let mut cur = Cursor::new(src);
    let start_pos = cur.pos();
    let mut blocks = Vec::new();
    let mut running_format: Option<EmbeddedFormat> = None;

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
            blocks.push(Block::Element(parse_heading(&mut cur, running_format)?));
            continue;
        }
        if is_titled_thematic_break_start(&cur) {
            blocks.push(Block::Element(parse_titled_thematic_break(
                &mut cur,
                running_format,
            )?));
            continue;
        }
        if is_thematic_break(&cur) {
            let item_start = cur.pos();
            consume_thematic_break(&mut cur);
            let mut el = typedmark_ast::Element::new(typedmark_ast::Sigil::Type("hr".to_string()));
            el.span = cur.span_from(item_start);
            blocks.push(Block::Element(el));
            continue;
        }
        if is_fenced_code_block_start(&cur) {
            blocks.push(Block::Element(parse_fenced_code_block(&mut cur)?));
            continue;
        }
        if let Some((ordered, ..)) = peek_list_marker(&cur)? {
            let items = parse_list(&mut cur, ordered, running_format)?;
            if !items.is_empty() {
                let list_span = typedmark_ast::Span::new(
                    items.first().unwrap().span.start,
                    items.last().unwrap().span.end,
                );
                blocks.push(Block::Element(typedmark_ast::Element::list(
                    ordered, items, list_span,
                )));
            }
            continue;
        }
        if cur.peek() == Some('<') && is_type_element_start(&cur) {
            let el = parse_element(&mut cur, running_format, true)?;
            if let Some(update) = config_format_update(&el) {
                running_format = update;
            }
            blocks.push(Block::Element(el));
            continue;
        }
        if cur.peek() == Some('@') && is_at_element_start(&cur) {
            let el = parse_element(&mut cur, running_format, true)?;
            if let Some(update) = config_format_update(&el) {
                running_format = update;
            }
            blocks.push(Block::Element(el));
            continue;
        }
        if cur.peek() == Some('$') && is_interp_start(&cur) {
            let el = parse_dollar_element(&mut cur)?;
            blocks.push(Block::Element(el));
            continue;
        }

        blocks.push(parse_paragraph(&mut cur, running_format)?);
    }

    let span = cur.span_from(start_pos);
    Ok(Document::new(blocks, span))
}

fn is_heading_start(cur: &Cursor) -> bool {
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

fn parse_paragraph(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Block> {
    let start_pos = cur.pos();
    let mut content = parse_inline_seq(cur, Stop::Paragraph, default_format, true)?;
    let span = cur.span_from(start_pos);
    if content.len() == 1 && matches!(content[0], Inline::Element(_)) {
        if let Inline::Element(mut el) = content.pop().unwrap() {
            if el.span == typedmark_ast::Span::default() {
                el.span = span;
            }
            return Ok(Block::Element(el));
        }
    }
    Ok(Block::Paragraph(Paragraph::new(content, span)))
}
