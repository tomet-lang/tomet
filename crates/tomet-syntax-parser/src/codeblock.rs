//! Parsing for fenced code blocks and raw/verbatim content spans.

use crate::error::Result;
use crate::value::{find_matching_bracket, find_matching_delimiter, skip_inline_ws};
use tomet_ast::{Element, Inline, Sigil, Span, Text, Value};
use tomet_lexar::Cursor;

pub(crate) fn is_fenced_code_block_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.eat_while(|c| c == '`').len() >= 3
}

pub(crate) fn parse_fenced_code_block(cur: &mut Cursor) -> Result<Element> {
    let start_pos = cur.pos();
    let fence_len = cur.eat_while(|c| c == '`').len();

    skip_inline_ws(cur);
    let info_start = cur.pos();
    cur.eat_while(|c| c != '\n' && c != '\r');
    let lang = cur
        .slice_from(info_start)
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string();
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }

    let body_start = cur.pos();
    let body_end;
    loop {
        if cur.is_eof() {
            body_end = cur.pos();
            break;
        }
        let line_start = cur.pos();
        let mut look = *cur;
        let run = look.eat_while(|c| c == '`').len();
        if run >= fence_len {
            skip_inline_ws(&mut look);
            if matches!(look.peek(), None | Some('\n') | Some('\r')) {
                body_end = line_start;
                cur.set_pos(look.pos());
                if matches!(cur.peek(), Some('\n') | Some('\r')) {
                    cur.bump();
                }
                break;
            }
        }
        cur.eat_while(|c| c != '\n' && c != '\r');
        if matches!(cur.peek(), Some('\n') | Some('\r')) {
            cur.bump();
        }
    }

    let mut code = cur.src()[body_start..body_end].to_string();
    if code.ends_with('\n') {
        code.pop();
    }

    let args = if lang.is_empty() {
        None
    } else {
        Some(Value::Map(vec![("lang".to_string(), Value::String(lang))]))
    };
    let content_span = Span::new(cur.position_at(body_start), cur.position_at(body_end));
    let mut el = Element::new(Sigil::Type("codeblock".to_string()))
        .with_span(cur.span_from(start_pos))
        .with_content(vec![Inline::Text(Text::new(code, content_span))]);
    el.args = args;
    Ok(el)
}

pub(crate) fn parse_raw_content(cur: &mut Cursor) -> Result<Vec<Inline>> {
    parse_raw_content_with(cur, find_matching_delimiter)
}

pub(crate) fn parse_verbatim_content(cur: &mut Cursor) -> Result<Vec<Inline>> {
    parse_raw_content_with(cur, find_matching_bracket)
}

fn parse_raw_content_with(
    cur: &mut Cursor,
    find_close: fn(&mut Cursor, char, char, usize) -> Result<usize>,
) -> Result<Vec<Inline>> {
    let group_start = cur.pos();
    if !cur.eat_str("[") {
        return Err(crate::value::err(cur, cur.pos(), "expected '['"));
    }
    let body_start = cur.pos();
    let body_end = find_close(cur, '[', ']', group_start)?;
    let raw = cur.src()[body_start..body_end].to_string();
    let span = Span::new(cur.position_at(body_start), cur.position_at(body_end));
    cur.set_pos(body_end);
    if !cur.eat_str("]") {
        return Err(crate::value::err(cur, cur.pos(), "expected ']'"));
    }
    Ok(vec![Inline::Text(Text::new(raw, span))])
}

pub(crate) fn is_codeblock(el: &Element) -> bool {
    matches!(&el.sigil, Sigil::Type(name) if name == "codeblock")
}

pub(crate) fn is_verbatim_content(el: &Element) -> bool {
    el.args
        .as_ref()
        .and_then(|a| a.get("content"))
        .and_then(|v| v.as_str())
        == Some("raw")
}
