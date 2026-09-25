//! Parsing for sections (`=[...]`, `=...`) and thematic breaks (`---`).

use crate::element::{parse_groups, parse_sugar_body};
use crate::error::Result;
use crate::inline::{Stop, parse_inline_seq};
use crate::value::{err, parse_value_at, skip_inline_ws, skip_ws_newlines_and_comments};
use tomet_ast::{Element, ElementValue, Inline, Placement, Section, Sigil, Value};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};

pub(crate) fn is_section_start(cur: &Cursor) -> bool {
    if cur.peek() != Some('=') {
        return false;
    }
    let mut look = *cur;
    look.eat_while(|c| c == '=');
    if crate::element::opens_group(look.peek()) {
        return true;
    }
    let had_ws = matches!(look.peek(), Some(' ') | Some('\t'));
    skip_inline_ws(&mut look);
    if matches!(look.peek(), None | Some('\n') | Some('\r'))
        || look.starts_with("//")
        || look.starts_with("/*")
    {
        return true;
    }
    had_ws
}

pub(crate) fn parse_section(cur: &mut Cursor) -> Result<Section> {
    let start_pos = cur.pos();
    let level = cur.eat_while(|c| c == '=').len();

    let had_ws = matches!(cur.peek(), Some(' ') | Some('\t'));
    skip_inline_ws(cur);

    let mut el = element_new(Sigil::named("section")).with_placement(Placement::Block);

    match cur.peek() {
        _ if crate::element::opens_group(cur.peek()) => {
            // The full form. Groups are read by the same code that reads
            // `@name`'s, so `=` takes `(args)`, `[content]` and `{value}`
            // in any order, and its content -- bracketed or `|`-marked --
            // may span lines.
            parse_groups(cur, &mut el, true)?;
            skip_inline_ws(cur);
            if cur.peek() == Some('=') {
                cur.eat_while(|c| c == '=');
                skip_inline_ws(cur);
                // Allow groups (e.g. `{ attrs }`) to follow decorative '='
                if crate::element::opens_group(cur.peek()) {
                    parse_groups(cur, &mut el, true)?;
                    skip_inline_ws(cur);
                    if cur.peek() == Some('=') {
                        cur.eat_while(|c| c == '=');
                        skip_inline_ws(cur);
                    }
                }
            }
        }
        _ if cur.starts_with("//") || cur.starts_with("/*") => {
            // A heading-less section with a trailing comment.
        }
        _ if had_ws && !matches!(cur.peek(), None | Some('\n') | Some('\r')) => {
            let (mut content, attrs) = parse_sugar_body(cur)?;
            trim_trailing_equals(&mut content);
            el.content = Some(content);
            el.value = attrs.map(ElementValue::from_map);
        }
        None | Some('\n') | Some('\r') => {
            // A heading-less section (`=` or `==` alone on its line).
            // `el.content` remains `None`, so `title` will default to empty.
        }
        _ => return Err(err(cur, cur.pos(), "expected '[' or a space after '='")),
    }

    skip_inline_ws(cur);
    if cur.starts_with("//") {
        crate::value::skip_line_comment(cur);
    }
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }

    let span = cur.span_from(start_pos);
    let title = el.content.unwrap_or_default();

    Ok(Section {
        level,
        title,
        args: el.args,
        value: el.value,
        connects: el.connects,
        blocks: Vec::new(),
        span,
    })
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

fn trim_trailing_equals(inlines: &mut Vec<Inline>) {
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        let trimmed = t.value.trim_end();
        if trimmed.ends_with('=') {
            let before_eq = trimmed.trim_end_matches('=');
            if before_eq.is_empty() || before_eq.ends_with(char::is_whitespace) {
                t.value = before_eq.trim_end().to_string();
                if t.value.is_empty() {
                    inlines.pop();
                }
            }
        }
    }
}
