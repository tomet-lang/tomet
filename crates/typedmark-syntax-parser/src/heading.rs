//! Parsing for headings (`#[...]`) and thematic breaks (`---`).

use crate::embedded_format::EmbeddedFormat;
use crate::error::Result;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::{
    err, parse_value_at, skip_inline_ws, skip_ws_and_newlines, skip_ws_newlines_and_comments,
};
use typedmark_ast::{Element, ElementValue, Sigil, Value};
use typedmark_lexar::Cursor;

pub(crate) fn parse_heading(
    cur: &mut Cursor,
    default_format: Option<EmbeddedFormat>,
) -> Result<Element> {
    let start_pos = cur.pos();
    let level = cur.eat_while(|c| c == '#').len() as u8;
    skip_inline_ws(cur);
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '[' after '#'"));
    }
    let content = parse_inline_seq(cur, Stop::Bracket(']'), default_format)?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    let checkpoint = cur.pos();
    skip_ws_and_newlines(cur);
    let mut attrs = if cur.peek() == Some('{') {
        Some(parse_braced_value(cur)?)
    } else {
        cur.set_pos(checkpoint);
        None
    };
    skip_inline_ws(cur);
    if cur.eat_str(":") {
        skip_inline_ws(cur);
        if cur.peek() == Some('{') {
            let connected_val = parse_braced_value(cur)?;
            attrs = merge_values(attrs.as_ref(), Some(&connected_val));
        } else if cur.peek() == Some('(') {
            let connected_args = parse_paren_value(cur)?;
            attrs = merge_values(attrs.as_ref(), Some(&connected_args));
        }
    }
    skip_inline_ws(cur);
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }
    let span = cur.span_from(start_pos);
    let mut el = Element::new(Sigil::At(Some("heading".to_string())));
    // Pre-existing quirk, preserved: a `#`-run longer than 255 silently
    // truncates here, same as before `Heading` was folded into `Element`.
    el.args = Some(Value::Int(level as i64));
    el.content = Some(content);
    el.value = attrs.map(ElementValue::Data);
    el.span = span;
    Ok(el)
}

pub(crate) fn parse_braced_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    let v = parse_value_at(cur)?;
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    Ok(v)
}

fn parse_paren_value(cur: &mut Cursor) -> Result<Value> {
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

pub(crate) fn is_thematic_break(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.eat_while(|c| c == '-').len() < 3 {
        return false;
    }
    skip_inline_ws(&mut look);
    matches!(look.peek(), None | Some('\n') | Some('\r'))
}

pub(crate) fn consume_thematic_break(cur: &mut Cursor) {
    cur.eat_while(|c| c == '-' || c == ' ' || c == '\t');
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }
}

pub(crate) fn is_titled_thematic_break_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.eat_while(|c| c == '-').len() < 3 {
        return false;
    }
    skip_inline_ws(&mut look);
    look.peek() == Some('[')
}

pub(crate) fn parse_titled_thematic_break(
    cur: &mut Cursor,
    default_format: Option<EmbeddedFormat>,
) -> Result<Element> {
    let start_pos = cur.pos();
    cur.eat_while(|c| c == '-');
    skip_inline_ws(cur);
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let title = parse_inline_seq(cur, Stop::Bracket(']'), default_format)?;
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    skip_inline_ws(cur);
    if cur.eat_while(|c| c == '-').len() < 3 {
        return Err(err(
            cur,
            cur.pos(),
            "expected 3 or more '-' to close the titled thematic break",
        ));
    }
    skip_inline_ws(cur);
    if !matches!(cur.peek(), None | Some('\n') | Some('\r')) {
        return Err(err(
            cur,
            cur.pos(),
            "unexpected trailing content after titled thematic break",
        ));
    }
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }
    let mut el = Element::new(Sigil::Type("hr".to_string())).with_span(cur.span_from(start_pos));
    el.content = Some(title);
    Ok(el)
}

pub(crate) fn merge_values(direct: Option<&Value>, connected: Option<&Value>) -> Option<Value> {
    match (direct, connected) {
        (None, None) => None,
        (Some(d), None) => Some(d.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(d), Some(c)) => Some(merge_values_inner(d, c)),
    }
}

fn merge_values_inner(direct: &Value, connected: &Value) -> Value {
    match (direct, connected) {
        (Value::Map(d_map), Value::Map(c_map)) => {
            let mut merged = c_map.clone();
            for (d_k, d_v) in d_map {
                if let Some(pos) = merged.iter().position(|(c_k, _)| c_k == d_k) {
                    let c_v = &merged[pos].1;
                    merged[pos].1 = merge_values_inner(d_v, c_v);
                } else {
                    merged.push((d_k.clone(), d_v.clone()));
                }
            }
            Value::Map(merged)
        }
        (Value::Seq(d_seq), Value::Seq(c_seq)) => {
            let mut merged = c_seq.clone();
            for item in d_seq {
                if !merged.contains(item) {
                    merged.push(item.clone());
                }
            }
            Value::Seq(merged)
        }
        (direct_val, _) => direct_val.clone(),
    }
}
