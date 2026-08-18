//! Parsing for list items (`-`, `-.`).

use crate::embedded_format::EmbeddedFormat;
use crate::error::Result;
use crate::heading::parse_braced_value;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::skip_inline_ws;
use typedmark_ast::ListItem;
use typedmark_lexar::Cursor;

pub(crate) fn eat_list_marker(cur: &mut Cursor) -> Option<(bool, Option<String>)> {
    let mut look = *cur;
    if look.bump() != Some('-') {
        return None;
    }
    let ordered = if look.peek() == Some('.') {
        look.bump();
        true
    } else {
        false
    };

    let has_ws = matches!(look.peek(), Some(' ') | Some('\t'));
    skip_inline_ws(&mut look);

    let marker = if look.peek() == Some('[') || look.peek() == Some('(') {
        let open_char = look.bump().unwrap();
        let close_char = if open_char == '[' { ']' } else { ')' };
        let mut inner = String::new();
        while let Some(c) = look.peek() {
            if c == close_char {
                look.bump();
                break;
            }
            if c == '\n' || c == '\r' {
                break;
            }
            inner.push(c);
            look.bump();
        }
        if look.peek() == Some(' ') || look.peek() == Some('\t') {
            Some(inner)
        } else {
            None
        }
    } else {
        None
    };

    if marker.is_none() && !has_ws {
        return None;
    }

    skip_inline_ws(&mut look);
    cur.set_pos(look.pos());
    Some((ordered, marker))
}

pub(crate) fn peek_list_marker(cur: &Cursor) -> Option<(bool, Option<String>)> {
    let mut look = *cur;
    eat_list_marker(&mut look)
}

fn peek_trailing_attrs(cur: &Cursor) -> Option<usize> {
    let mut look = *cur;
    let mut last_brace_pos = None;
    while !look.is_eof() && look.peek() != Some('\n') && look.peek() != Some('\r') {
        if look.peek() == Some('{') {
            last_brace_pos = Some(look.pos());
        }
        look.bump();
    }
    if let Some(pos) = last_brace_pos {
        let mut test_cur = *cur;
        test_cur.set_pos(pos);
        if parse_braced_value(&mut test_cur).is_ok() {
            skip_inline_ws(&mut test_cur);
            if matches!(test_cur.peek(), None | Some('\n') | Some('\r')) {
                return Some(pos);
            }
        }
    }
    None
}

pub(crate) fn parse_list(
    cur: &mut Cursor,
    ordered: bool,
    default_format: Option<EmbeddedFormat>,
) -> Result<Vec<ListItem>> {
    let mut items = Vec::new();
    while let Some((item_ordered, marker)) = peek_list_marker(cur) {
        if item_ordered != ordered {
            break;
        }
        let item_start = cur.pos();
        eat_list_marker(cur);

        let (content, attrs) = if let Some(brace_pos) = peek_trailing_attrs(cur) {
            let content = parse_inline_seq(cur, Stop::Offset(brace_pos), default_format)?;
            let attrs = parse_braced_value(cur)?;
            (content, Some(attrs))
        } else {
            let content = parse_inline_seq(cur, Stop::Line, default_format)?;
            (content, None)
        };

        if cur.peek() == Some('\n') {
            cur.bump();
        }
        let span = cur.span_from(item_start);
        items.push(ListItem::new(content, marker, attrs, span));
    }
    Ok(items)
}
