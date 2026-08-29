//! Parsing for list items (`-`, `-.`).
//!
//! A list marker's optional bracket form is `(...)`: a real `Value` parsed
//! with the same grammar as an element's `(args)` (see
//! `crate::element::parse_paren_value`), normalized against the builtin
//! `"marker"` positional key by `tomet-semantics::positional`.

use crate::element::parse_paren_value;
use crate::embedded_format::EmbeddedFormat;
use crate::error::Result;
use crate::heading::parse_braced_value;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::skip_inline_ws;
use tomet_ast::{Element, Value};
use tomet_lexar::Cursor;

pub(crate) fn eat_list_marker_with_indent(
    cur: &mut Cursor,
) -> Result<Option<(usize, bool, Option<Value>)>> {
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
        return Ok(None);
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

    let mut marker = None;
    let mut found_bracket = false;

    if look.peek() == Some('(') {
        let mut probe = look;
        let value = parse_paren_value(&mut probe)?;
        if matches!(probe.peek(), Some(' ') | Some('\t')) {
            marker = Some(value);
            found_bracket = true;
            look = probe;
        }
    }

    if !found_bracket && !has_ws {
        return Ok(None);
    }

    skip_inline_ws(&mut look);
    cur.set_pos(look.pos());
    Ok(Some((indent, ordered, marker)))
}

pub(crate) fn eat_list_marker(cur: &mut Cursor) -> Result<Option<(bool, Option<Value>)>> {
    Ok(eat_list_marker_with_indent(cur)?.map(|(_, ordered, marker)| (ordered, marker)))
}

pub(crate) fn peek_list_marker_with_indent(
    cur: &Cursor,
) -> Result<Option<(usize, bool, Option<Value>)>> {
    let mut look = *cur;
    eat_list_marker_with_indent(&mut look)
}

pub(crate) fn peek_list_marker(cur: &Cursor) -> Result<Option<(bool, Option<Value>)>> {
    let mut look = *cur;
    eat_list_marker(&mut look)
}

/// Byte offset of a `:` immediately (only inline whitespace between)
/// preceding `brace_pos`, if there is one -- the colon-connect marker
/// that dedicates this item's trailing `{...}` to the item itself
/// rather than to whatever element precedes it (see
/// `element::parse_element`'s `allow_colon_connect` doc comment).
/// Byte-indexed but UTF-8 safe: only ever steps back over bytes it has
/// just confirmed are the ASCII space/tab/colon it's looking for, so it
/// never lands on, or reads across, a multi-byte character's interior.
fn connect_colon_pos(src: &str, brace_pos: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = brace_pos;
    while i > 0 && matches!(bytes[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    if i > 0 && bytes[i - 1] == b':' {
        Some(i - 1)
    } else {
        None
    }
}

/// Finds this item's own trailing `{attrs}`, if the rest of the line
/// has one. Returns `(content_stop, brace_pos)`: `content_stop` is
/// where the item's own inline *content* parsing must stop -- normally
/// the same as `brace_pos` (a bare, colon-less `{}` is always claimed
/// by an element instead, so content parsing runs right up to it
/// regardless of whether it turns out to belong to the item or not),
/// but the position of a preceding colon-connect marker instead when
/// there is one, so that marker isn't left dangling as ordinary
/// trailing text once `allow_colon_connect: false` stops an inner
/// element from consuming it itself.
fn peek_trailing_attrs(
    cur: &Cursor,
    default_format: Option<EmbeddedFormat>,
) -> Option<(usize, usize)> {
    let mut look = *cur;
    let mut last_brace_pos = None;
    while !look.is_eof() && look.peek() != Some('\n') && look.peek() != Some('\r') {
        if look.peek() == Some('{') {
            last_brace_pos = Some(look.pos());
        }
        look.bump();
    }
    let brace_pos = last_brace_pos?;
    let content_stop = connect_colon_pos(cur.src(), brace_pos).unwrap_or(brace_pos);

    // Confirm inline content parsing, stopped at `content_stop`,
    // actually lands exactly there rather than overshooting it. A bare,
    // colon-less trailing group is still always claimed by an element
    // regardless of `allow_colon_connect` (`@meta(format:yaml) {...}`
    // is real, existing usage that must keep working) -- so this line's
    // one `{` can still belong to an inner element instead of the item,
    // and `Stop::Offset` alone can't be trusted to have actually
    // stopped parsing there: `parse_inline_seq` only checks its target
    // *between* separate line items, not while a single element is
    // mid-way through claiming one more trailing group -- so it can
    // walk straight past `content_stop` without ever noticing.
    // Speculatively parsing here (the result is discarded either way --
    // `parse_list_internal` reparses for real once this confirms the
    // guess) is the only way to know without duplicating
    // `parse_element`'s own claiming logic.
    let mut probe = *cur;
    let landed_at_pos = matches!(
        parse_inline_seq(&mut probe, Stop::Offset(content_stop), default_format, false),
        Ok(_) if probe.pos() == content_stop
    );
    if !landed_at_pos {
        return None;
    }

    let mut test_cur = *cur;
    test_cur.set_pos(brace_pos);
    if parse_braced_value(&mut test_cur).is_ok() {
        skip_inline_ws(&mut test_cur);
        if matches!(test_cur.peek(), None | Some('\n') | Some('\r')) {
            return Some((content_stop, brace_pos));
        }
    }
    None
}

pub(crate) fn parse_list(
    cur: &mut Cursor,
    ordered: bool,
    default_format: Option<EmbeddedFormat>,
) -> Result<Vec<Element>> {
    parse_list_internal(cur, ordered, 0, default_format)
}

fn parse_list_internal(
    cur: &mut Cursor,
    ordered: bool,
    min_indent: usize,
    default_format: Option<EmbeddedFormat>,
) -> Result<Vec<Element>> {
    let mut items = Vec::new();
    while let Some((indent, item_ordered, marker)) = peek_list_marker_with_indent(cur)? {
        if indent < min_indent || item_ordered != ordered {
            break;
        }
        let item_start = cur.pos();
        eat_list_marker_with_indent(cur)?;

        let (content, attrs) = if let Some((content_stop, brace_pos)) =
            peek_trailing_attrs(cur, default_format)
        {
            let content = parse_inline_seq(cur, Stop::Offset(content_stop), default_format, false)?;
            // `content_stop` is the colon-connect marker's position when
            // there is one (see `peek_trailing_attrs`'s doc comment),
            // short of `brace_pos` -- jump the rest of the way past it
            // and its surrounding whitespace, neither of which need to
            // survive as an AST node of their own.
            cur.set_pos(brace_pos);
            let attrs = parse_braced_value(cur)?;
            (content, Some(attrs))
        } else {
            let content = parse_inline_seq(cur, Stop::Line, default_format, false)?;
            (content, None)
        };

        if cur.peek() == Some('\n') {
            cur.bump();
        }

        let mut children = Vec::new();
        while let Some((next_indent, next_ordered, ..)) = peek_list_marker_with_indent(cur)? {
            if next_indent > indent {
                let sub_items =
                    parse_list_internal(cur, next_ordered, next_indent, default_format)?;
                if !sub_items.is_empty() {
                    let list_span = tomet_ast::Span::new(
                        sub_items.first().unwrap().span.start,
                        sub_items.last().unwrap().span.end,
                    );
                    children.push(tomet_ast::Block::Element(Element::list(
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
        items.push(Element::list_item(content, marker, attrs, children, span));
    }
    Ok(items)
}
