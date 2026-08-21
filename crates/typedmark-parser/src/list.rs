//! Parsing for list items (`-`, `-.`).

use crate::embedded_format::EmbeddedFormat;
use crate::error::Result;
use crate::heading::parse_braced_value;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::skip_inline_ws;
use typedmark_ast::ListItem;
use typedmark_lexar::Cursor;

pub(crate) fn eat_list_marker_with_indent(
    cur: &mut Cursor,
) -> Option<(usize, bool, Option<String>)> {
    let mut look = *cur;
    let mut indent = 0;
    while matches!(look.peek(), Some(' ') | Some('\t')) {
        if look.peek() == Some('\t') {
            indent += 4;
        } else {
            indent += 1;
        }
        look.bump();
    }
    if look.peek() != Some('-') {
        return None;
    }
    look.bump();
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
    Some((indent, ordered, marker))
}

pub(crate) fn eat_list_marker(cur: &mut Cursor) -> Option<(bool, Option<String>)> {
    eat_list_marker_with_indent(cur).map(|(_, ordered, marker)| (ordered, marker))
}

pub(crate) fn peek_list_marker_with_indent(cur: &Cursor) -> Option<(usize, bool, Option<String>)> {
    let mut look = *cur;
    eat_list_marker_with_indent(&mut look)
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
    parse_list_internal(cur, ordered, 0, default_format)
}

fn parse_list_internal(
    cur: &mut Cursor,
    ordered: bool,
    min_indent: usize,
    default_format: Option<EmbeddedFormat>,
) -> Result<Vec<ListItem>> {
    let mut items = Vec::new();
    while let Some((indent, item_ordered, marker)) = peek_list_marker_with_indent(cur) {
        if indent < min_indent || item_ordered != ordered {
            break;
        }
        let item_start = cur.pos();
        eat_list_marker_with_indent(cur);

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

        let mut children = Vec::new();
        while let Some((next_indent, next_ordered, _)) = peek_list_marker_with_indent(cur) {
            if next_indent > indent {
                let sub_items =
                    parse_list_internal(cur, next_ordered, next_indent, default_format)?;
                if !sub_items.is_empty() {
                    let list_span = typedmark_ast::Span::new(
                        sub_items.first().unwrap().span.start,
                        sub_items.last().unwrap().span.end,
                    );
                    children.push(typedmark_ast::Block::List(typedmark_ast::List::new(
                        next_ordered,
                        sub_items,
                        list_span,
                    )));
                }
            } else {
                break;
            }
        }

        let span = cur.span_from(item_start);
        items.push(ListItem::with_children(
            content, marker, attrs, children, span,
        ));
    }
    Ok(items)
}
