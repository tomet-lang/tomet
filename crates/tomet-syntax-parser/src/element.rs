//! Parsing for typed elements (`<T>`, `@name`, bare elements, colon connect syntax).

use crate::error::Result;
use crate::fence::{is_fence_start, parse_fence};
use crate::heading::merge_values;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::{
    POSITIONAL_ENTRY_KEY, eat_name, err, is_name_start_at, parse_one_entry, parse_value_at,
    skip_block_comment, skip_inline_ws, skip_line_comment, skip_ws_newlines_and_comments,
};
use tomet_ast::{Element, ElementValue, Entry, Inline, Sigil, Value};
use tomet_lexer::Cursor;
use tomet_tree::element_new;

/// Whether `cur` starts an inline element (`@name`, or a bare `@`).
///
/// An `@` only introduces an element when a group or a `:` connect
/// follows. That is what keeps `me@example.com` and a lone `@foo` in prose
/// as plain text, and it is the same fall-back-to-text rule `#` uses
/// below.
pub(crate) fn is_inline_element_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('@') {
        return false;
    }
    let _ = eat_name(&mut look);
    skip_lookahead_gap(&mut look);
    matches!(look.peek(), Some('(') | Some('[') | Some('{') | Some(':')) || is_fence_start(&look)
}

/// Whether `cur` starts a block element (`#name`).
///
/// The name must follow the `#` run immediately -- no space. `# heading`
/// therefore stays prose, which is what keeps Markdown-style headings and
/// shell/YAML comments inside the docs from being reinterpreted.
///
/// Unlike `@`, a block element may also be followed by nothing at all: a
/// `#name` alone on its line is an element. It then fails later, in
/// `tomet-semantics`, as an unknown bare name -- which is the intended
/// report, and the reason no hashtag syntax is being introduced.
pub(crate) fn is_block_element_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('#') {
        return false;
    }
    if !is_name_start_at(&look) {
        return false;
    }
    let _ = eat_name(&mut look);
    skip_lookahead_gap(&mut look);
    matches!(look.peek(), Some('(') | Some('[') | Some('{') | Some(':'))
        || is_fence_start(&look)
        || matches!(look.peek(), None | Some('\n') | Some('\r'))
}

fn skip_element_gap(cur: &mut Cursor) -> u8 {
    let mut newlines = 0u8;
    loop {
        skip_inline_ws(cur);
        if cur.starts_with("//") {
            skip_line_comment(cur);
            continue;
        }
        if cur.starts_with("/*") {
            let mut look = *cur;
            if skip_block_comment(&mut look).is_ok() {
                *cur = look;
                continue;
            }
            break;
        }
        match cur.peek() {
            Some('\n') | Some('\r') => {
                cur.bump();
                newlines += 1;
                if newlines > 1 {
                    break;
                }
            }
            _ => break,
        }
    }
    newlines
}

fn skip_lookahead_gap(cur: &mut Cursor) {
    skip_element_gap(cur);
}

/// `allow_colon_connect` gates the `:(...)`/`:{...}` "connect" branch
/// below (a bare, colon-less trailing group is *always* claimed
/// regardless -- see `docs/spec/syntax.tmt`'s
/// `@meta(format:yaml) {...}` example, real usage this must keep
/// working). List items pass `false` for the single top-level element
/// they parse as their own content (`list.rs::parse_list_internal`):
/// a list item has its own optional trailing `{attrs}`, and without
/// this, a colon-prefixed group meant for the *item* (`- @link(ref:x)
/// :{id:breakfast}`) always got silently claimed by `@link` instead --
/// by the time `list.rs` got a turn, the group was already gone, with
/// nothing left at the position it expected to still find one at (see
/// `list_item_ending_in_an_element_does_not_error_on_a_trailing_brace`'s
/// history for the crash this used to cause before the item-attrs guess
/// was made speculative). `docs/spec/syntax.tmt`'s
/// `##[ コネクト ]` section had flagged exactly this shape (`- ()
/// xxxxxx :{}`) as an unimplemented idea for attaching a group to the
/// *enclosing* construct rather than the nearest element -- this is
/// that, scoped narrowly to where the ambiguity actually is. Every
/// other caller (top-level block elements, content nested inside an
/// already-bracketed group, paragraph prose) passes `true`, unchanged:
/// none of those have a competing attrs slot of their own to lose the
/// group to, so `<id:taskA>:{...}`-style remote connect (see
/// `remote_id_target_element_supports_colon_connection`) keeps working
/// exactly as before there.
pub(crate) fn parse_element(cur: &mut Cursor, allow_colon_connect: bool) -> Result<Element> {
    let start_pos = cur.pos();
    let sigil = if cur.peek() == Some('#') {
        cur.bump();
        match eat_name(cur) {
            Some(name) => Sigil::Block(name),
            None => return Err(err(cur, cur.pos(), "expected an element name after '#'")),
        }
    } else {
        if !cur.eat_str("@") {
            return Err(err(cur, cur.pos(), "expected '@'"));
        }
        Sigil::Inline(eat_name(cur))
    };

    let mut el = element_new(sigil);
    loop {
        let checkpoint = cur.pos();
        let newlines = skip_element_gap(cur);
        if newlines <= 1 {
            if allow_colon_connect && cur.eat_str(":") {
                skip_inline_ws(cur);
                match cur.peek() {
                    Some('(') => {
                        let conn_args = parse_paren_value(cur)?;
                        el.args = merge_values(el.args.as_ref(), Some(&conn_args));
                        continue;
                    }
                    Some('{') => {
                        let conn_val = parse_value_group(cur)?;
                        // Connect merges pairs into an existing group; a
                        // group holding elements is replaced wholesale,
                        // as there is nothing to merge key-wise.
                        let merged = match (&el.value, &conn_val) {
                            (Some(existing), ElementValue::Group(_))
                                if !existing.has_children() && !conn_val.has_children() =>
                            {
                                merge_values(
                                    existing.as_data().as_ref(),
                                    conn_val.as_data().as_ref(),
                                )
                                .map(ElementValue::from_map)
                            }
                            _ => None,
                        };
                        el.value = Some(merged.unwrap_or(conn_val));
                        continue;
                    }
                    _ => {}
                }
            }
            match cur.peek() {
                Some('(') if el.args.is_none() => {
                    el.args = Some(parse_paren_value(cur)?);
                    continue;
                }
                Some('[') if el.content.is_none() => {
                    el.content = Some(parse_content(cur)?);
                    continue;
                }
                Some('{') if el.value.is_none() => {
                    el.value = Some(parse_value_group(cur)?);
                    continue;
                }
                // A `+++` fence is exclusive with `[content]` and
                // `{value}`: it *is* the body, captured verbatim.
                _ if el.value.is_none() && el.content.is_none() && is_fence_start(cur) => {
                    el.value = Some(ElementValue::Raw(parse_fence(cur)?));
                    continue;
                }
                _ => {}
            }
        }
        cur.set_pos(checkpoint);
        break;
    }
    el.span = cur.span_from(start_pos);
    Ok(el)
}

pub(crate) fn parse_paren_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("(") {
        return Err(err(cur, cur.pos(), "expected '('"));
    }
    skip_ws_newlines_and_comments(cur);
    let v = if cur.peek() == Some(')') {
        Value::Map(Vec::new())
    } else {
        parse_value_at(cur)?
    };
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str(")") {
        return Err(err(cur, cur.pos(), "expected ')'"));
    }
    Ok(v)
}

fn parse_content(cur: &mut Cursor) -> Result<Vec<Inline>> {
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let content = parse_inline_seq(cur, Stop::Bracket(']'), true)?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    Ok(content)
}

/// Parses a `{...}` value group.
///
/// `{}` is always data: entries are read uniformly, each either a
/// `key: value` pair or a nested element, in source order. Nothing here
/// consults the enclosing element's name -- which is what lets
/// `#links{ (1)[a] note:x (2)[b] }` parse at all. It previously either
/// errored or, worse, swallowed the elements into a scalar string,
/// depending on which came first.
///
/// A non-map body (`{[1,2,3]}`, `{"str"}`, `{bare}`) is rejected. Those had
/// no uniform-entry spelling, and the `+++` fence now covers the case they
/// served -- `#meta(format:json)+++ [1,2,3] +++`.
pub(crate) fn parse_value_group(cur: &mut Cursor) -> Result<ElementValue> {
    let group_start = cur.pos();
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    let mut entries = Vec::new();
    loop {
        skip_ws_newlines_and_comments(cur);
        match cur.peek() {
            None => return Err(err(cur, group_start, "unterminated '{', expected '}'")),
            Some('}') => break,
            Some('(') => entries.push(Entry::Element(parse_bare_element(cur)?)),
            _ => {
                let (key, value) = parse_one_entry(cur)?;
                if key == POSITIONAL_ENTRY_KEY {
                    return Err(err(
                        cur,
                        group_start,
                        "a '{...}' group holds 'key: value' entries or elements; \
                         write a bare value in '(args)', or use a '+++' fence",
                    ));
                }
                entries.push(Entry::Pair(key, value));
            }
        }
        // Entries may be separated by `,` or just by whitespace.
        skip_ws_newlines_and_comments(cur);
        if cur.peek() == Some(',') {
            cur.bump();
        }
    }
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    Ok(ElementValue::Group(entries))
}

fn parse_bare_element(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    let args = parse_paren_value(cur)?;
    let mut el = element_new(Sigil::Bare);
    el.args = Some(args);
    let checkpoint = cur.pos();
    skip_inline_ws(cur);
    if cur.peek() == Some('[') {
        el.content = Some(parse_content(cur)?);
    } else {
        cur.set_pos(checkpoint);
    }
    el.span = cur.span_from(start_pos);
    Ok(el)
}
