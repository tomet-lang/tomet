//! Parsing for list items (`-`, `-.`).
//!
//! A list marker's optional bracket form is `(...)`: a real `Value` parsed
//! with the same grammar as an element's `(args)` (see
//! `crate::element::parse_paren_value`), normalized against the builtin
//! `"marker"` positional key by `tomet-semantics::positional`.

use crate::element::{parse_groups, parse_paren_value, parse_sugar_body};
use crate::error::Result;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::skip_inline_ws;
use tomet_ast::{Element, Inline, Sigil, Text, Value};
use tomet_lexer::Cursor;
use tomet_tree::{element_list, element_list_item, element_new};

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
        // `- (x) content` and `- (x)[ content ]` are the same element with
        // the same args; only the sugar needs a space to separate the
        // marker from the text that follows it.
        if matches!(probe.peek(), Some(' ') | Some('\t') | Some('[') | Some('{')) {
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


pub(crate) fn parse_list(cur: &mut Cursor, ordered: bool) -> Result<Vec<Element>> {
    parse_list_internal(cur, ordered, 0)
}

fn parse_list_internal(cur: &mut Cursor, ordered: bool, min_indent: usize) -> Result<Vec<Element>> {
    let mut items = Vec::new();
    while let Some((indent, item_ordered, marker)) = peek_list_marker_with_indent(cur)? {
        if indent < min_indent || item_ordered != ordered {
            break;
        }
        let item_start = cur.pos();
        eat_list_marker_with_indent(cur)?;

        let (content, attrs) = if matches!(cur.peek(), Some('[') | Some('{')) {
            // The full form: `- ()[ content ]{value}`. Groups are read by
            // the same code that reads `@name`'s, so `[content]` stops at
            // its closing bracket and may span lines. The bracket-less
            // sugar below stays single-line, which is the whole point of
            // requiring a group to spread out.
            let mut item = element_new(Sigil::Bare);
            parse_groups(cur, &mut item, false)?;
            let attrs = item.value.and_then(|v| v.as_data());
            let mut content = item.content.unwrap_or_default();
            // Text after the groups belongs to the item, the way
            // `@x[T] content` keeps both halves in one paragraph. Letting
            // it fall out as a block of its own would silently move a
            // sentence out of the list it was written in.
            let after_groups = cur.pos();
            skip_inline_ws(cur);
            if !matches!(cur.peek(), None | Some('\n')) {
                // `parse_inline_seq` trims its own edges, which is right
                // for a standalone sequence and wrong when appending to
                // one: the space that separated the group from the text
                // has to be put back by hand. Not when the group was
                // empty, though -- `- [ ] text` has nothing to separate
                // the text from.
                if cur.pos() != after_groups && !content.is_empty() {
                    content.push(Inline::Text(Text {
                        value: " ".to_string(),
                        span: cur.span_from(after_groups),
                    }));
                }
                content.extend(parse_inline_seq(cur, Stop::Line, false)?);
            }
            (content, attrs)
        } else {
            // The bracket-less sugar, shared with `#`: one line, plus this
            // line's own trailing `{attrs}`.
            parse_sugar_body(cur)?
        };

        if cur.peek() == Some('\n') {
            cur.bump();
        }

        let mut children = Vec::new();
        while let Some((next_indent, next_ordered, ..)) = peek_list_marker_with_indent(cur)? {
            if next_indent > indent {
                let sub_items = parse_list_internal(cur, next_ordered, next_indent)?;
                if !sub_items.is_empty() {
                    let list_span = sub_items
                        .first()
                        .unwrap()
                        .span
                        .union(&sub_items.last().unwrap().span);
                    children.push(tomet_ast::Block::Element(element_list(
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
        items.push(element_list_item(content, marker, attrs, children, span));
    }
    Ok(items)
}
