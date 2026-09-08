//! Parsing for headings (`#[...]`) and thematic breaks (`---`).

use crate::element::{parse_groups, parse_sugar_body};
use crate::error::Result;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::{err, parse_value_at, skip_inline_ws, skip_ws_newlines_and_comments};
use tomet_ast::{Element, ElementValue, Placement, Sigil, Value};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};

pub(crate) fn parse_heading(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    let level = cur.eat_while(|c| c == '#').len() as u8;

    // A `#` run must be followed by a group, or by whitespace and then the
    // sugar's content. `#tag` is neither, and stays prose.
    let had_ws = matches!(cur.peek(), Some(' ') | Some('\t'));
    skip_inline_ws(cur);

    let mut el = element_new(Sigil::named("heading")).with_placement(Placement::Block);

    match cur.peek() {
        _ if crate::element::opens_group(cur.peek()) => {
            // The full form. Groups are read by the same code that reads
            // `@name`'s, so `#` takes `(args)`, `[content]` and `{value}`
            // in any order, and its content -- bracketed or `|`-marked --
            // may span lines.
            parse_groups(cur, &mut el, true)?;
        }
        _ if had_ws => {
            let (content, attrs) = parse_sugar_body(cur)?;
            el.content = Some(content);
            el.value = attrs.map(ElementValue::from_map);
        }
        _ => return Err(err(cur, cur.pos(), "expected '[' or a space after '#'")),
    }

    skip_inline_ws(cur);
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }

    // The level is the `#` run, and it lands in `args` under the builtin
    // positional key that `tomet-semantics::heading_level` reads. An
    // explicit `(args)` group merges *around* it rather than replacing it:
    // `#(id: x)[ y ]` is a level-1 heading that also carries `id`.
    // Pre-existing quirk, preserved: a `#`-run longer than 255 silently
    // truncates here, same as before `Heading` was folded into `Element`.
    let level_args = Value::Map(vec![("level".into(), Value::Int(level as i64))]);
    el.args = match el.args.take() {
        Some(explicit) => merge_values(Some(&level_args), Some(&explicit)),
        None => Some(Value::Int(level as i64)),
    };

    el.span = cur.span_from(start_pos);
    Ok(el)
}

pub(crate) fn parse_braced_value(cur: &mut Cursor) -> Result<Value> {
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    skip_ws_newlines_and_comments(cur);
    // `parse_value_at`'s own fallback (`parse_map_body_or_scalar`)
    // deliberately errors ("expected a value") on an immediately-
    // closing bracket, since *that* function is also used for sequence
    // items and single-value positions where an empty body is never
    // valid. A whole `{}`/`()` *group*, though, is: `element.rs`'s
    // `parse_value_group` and `parse_paren_value` both special-case it
    // into an empty `Value::Map` before ever calling
    // `parse_value_at` -- this mirrors that (this function's callers,
    // heading/list-item attrs, are exactly that same "whole group"
    // position, not a sequence item).
    let v = if cur.peek() == Some('}') {
        Value::Map(Vec::new())
    } else {
        parse_value_at(cur)?
    };
    skip_ws_newlines_and_comments(cur);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
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

pub(crate) fn parse_titled_thematic_break(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    cur.eat_while(|c| c == '-');
    skip_inline_ws(cur);
    if !cur.eat_str("[") {
        return Err(err(cur, cur.pos(), "expected '['"));
    }
    let title = parse_inline_seq(cur, Stop::Bracket(']'), true)?;
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
    let mut el = element_new(Sigil::named("hr"))
        .with_placement(Placement::Block)
        .with_span(cur.span_from(start_pos));
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
