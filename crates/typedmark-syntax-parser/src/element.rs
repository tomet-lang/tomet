//! Parsing for typed elements (`<T>`, `@name`, bare elements, colon connect syntax).

use crate::codeblock::{
    is_codeblock, is_verbatim_content, parse_raw_content, parse_verbatim_content,
};
use crate::embedded_format::{EmbeddedFormat, parse_embedded_format_value};
use crate::error::Result;
use crate::heading::merge_values;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::{
    eat_ident, err, is_ident_char, parse_value_at, skip_block_comment, skip_inline_ws,
    skip_line_comment, skip_ws_newlines_and_comments,
};
use typedmark_ast::{Element, ElementValue, Inline, Sigil, Value};
use typedmark_lexar::Cursor;

pub(crate) fn is_type_element_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('<') {
        return false;
    }
    let ident = look.eat_while(|c| c != '>' && c != '\n' && c != '\r');
    if ident.is_empty() {
        return false;
    }
    if look.bump() != Some('>') {
        return false;
    }
    skip_lookahead_gap(&mut look);
    matches!(look.peek(), Some('(') | Some('[') | Some('{') | Some(':'))
}

pub(crate) fn is_at_element_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('@') {
        return false;
    }
    look.eat_while(is_ident_char);
    skip_lookahead_gap(&mut look);
    matches!(look.peek(), Some('(') | Some('[') | Some('{') | Some(':'))
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
/// regardless -- see `docs/ja/specifications/syntax.tm`'s
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
/// was made speculative). `docs/ja/specifications/syntax.tm`'s
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
pub(crate) fn parse_element(
    cur: &mut Cursor,
    default_format: Option<EmbeddedFormat>,
    allow_colon_connect: bool,
) -> Result<Element> {
    let start_pos = cur.pos();
    let sigil = if cur.peek() == Some('<') {
        cur.bump();
        let name = cur
            .eat_while(|c| c != '>' && c != '\n' && c != '\r')
            .to_string();
        if name.is_empty() {
            return Err(err(cur, cur.pos(), "expected a type name after '<'"));
        }
        if !cur.eat_str(">") {
            return Err(err(cur, cur.pos(), "expected '>' after type name"));
        }
        Sigil::Type(name)
    } else {
        if !cur.eat_str("@") {
            return Err(err(cur, cur.pos(), "expected '@'"));
        }
        let name = eat_ident(cur).to_string();
        if name.is_empty() {
            Sigil::At(None)
        } else {
            Sigil::At(Some(name))
        }
    };

    let mut el = Element::new(sigil);
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
                        let format = match local_format_key(&el) {
                            Some(explicit) => explicit,
                            None => default_format,
                        };
                        let conn_val = match format {
                            Some(format) => {
                                ElementValue::Data(parse_embedded_format_value(cur, format)?)
                            }
                            None => parse_value_group(cur, default_format)?,
                        };
                        if let ElementValue::Data(conn_v) = conn_val {
                            let direct_v = match &el.value {
                                Some(ElementValue::Data(v)) => Some(v),
                                _ => None,
                            };
                            if let Some(merged) = merge_values(direct_v, Some(&conn_v)) {
                                el.value = Some(ElementValue::Data(merged));
                            }
                        } else {
                            el.value = Some(conn_val);
                        }
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
                Some('(') => {
                    return Err(err(cur, cur.pos(), "duplicate '(' group"));
                }
                Some('[') if el.content.is_none() => {
                    el.content = Some(if is_codeblock(&el) {
                        parse_raw_content(cur)?
                    } else if is_verbatim_content(&el) {
                        parse_verbatim_content(cur)?
                    } else {
                        parse_content(cur, default_format)?
                    });
                    continue;
                }
                Some('[') => {
                    return Err(err(cur, cur.pos(), "duplicate '[' group"));
                }
                Some('{') if el.value.is_none() => {
                    let format = match local_format_key(&el) {
                        Some(explicit) => explicit,
                        None => default_format,
                    };
                    el.value = Some(match format {
                        Some(format) => {
                            ElementValue::Data(parse_embedded_format_value(cur, format)?)
                        }
                        None => parse_value_group(cur, default_format)?,
                    });
                    continue;
                }
                Some('{') => {
                    return Err(err(cur, cur.pos(), "duplicate '{' group"));
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

fn parse_content(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Vec<Inline>> {
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let content = parse_inline_seq(cur, Stop::Bracket(']'), default_format, true)?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    Ok(content)
}

fn is_format_target_element(el: &Element) -> bool {
    matches!(&el.sigil, Sigil::At(Some(name)) | Sigil::Type(name) if name == "meta" || name == "config")
}

pub(crate) fn local_format_key(el: &Element) -> Option<Option<EmbeddedFormat>> {
    let args = el.args.as_ref()?;
    match args {
        Value::Map(entries) => entries.iter().find_map(|(key, v)| {
            if key != "format" {
                return None;
            }
            Some(match v {
                Value::String(tag) => EmbeddedFormat::from_tag(tag),
                _ => None,
            })
        }),
        Value::String(tag) if is_format_target_element(el) => Some(EmbeddedFormat::from_tag(tag)),
        _ => None,
    }
}

fn is_config(el: &Element) -> bool {
    matches!(&el.sigil, Sigil::At(Some(name)) if name == "config")
}

pub(crate) fn config_format_update(el: &Element) -> Option<Option<EmbeddedFormat>> {
    if !is_config(el) {
        return None;
    }
    local_format_key(el)
}

pub(crate) fn parse_value_group(
    cur: &mut Cursor,
    default_format: Option<EmbeddedFormat>,
) -> Result<ElementValue> {
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    skip_ws_newlines_and_comments(cur);
    let result = if cur.peek() == Some('(') {
        let mut children = Vec::new();
        loop {
            skip_ws_newlines_and_comments(cur);
            if cur.peek() != Some('(') {
                break;
            }
            children.push(parse_bare_element(cur, default_format)?);
        }
        ElementValue::Children(children)
    } else if cur.peek() == Some('}') {
        ElementValue::Data(Value::Map(Vec::new()))
    } else {
        ElementValue::Data(parse_value_at(cur)?)
    };
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    Ok(result)
}

fn parse_bare_element(cur: &mut Cursor, default_format: Option<EmbeddedFormat>) -> Result<Element> {
    let start_pos = cur.pos();
    let args = parse_paren_value(cur)?;
    let mut el = Element::new(Sigil::Bare);
    el.args = Some(args);
    let checkpoint = cur.pos();
    skip_inline_ws(cur);
    if cur.peek() == Some('[') {
        el.content = Some(parse_content(cur, default_format)?);
    } else {
        cur.set_pos(checkpoint);
    }
    el.span = cur.span_from(start_pos);
    Ok(el)
}
