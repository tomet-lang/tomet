//! Parsing for `Value`: map bodies (`key: value`), `[..]` sequences,
//! `"..."` strings, and bare scalars. Shared by the data-only entry point
//! (`parse_value`) and by `document.rs` for `(input)` groups and
//! data-shaped `{value}` groups.

use crate::error::{Error, Result};
use typedmark_ast::Value;
use typedmark_lexar::Cursor;

/// Parse an entire source string as one `Value` (a data-only `.tm`
/// document -- no headings, prose, or elements).
pub fn parse_value(src: &str) -> Result<Value> {
    let mut cur = Cursor::new(src);
    let value = parse_value_at(&mut cur)?;
    skip_ws_and_newlines(&mut cur);
    if !cur.is_eof() {
        return Err(err(&cur, cur.pos(), "unexpected trailing content"));
    }
    Ok(value)
}

pub(crate) fn err(cur: &Cursor, pos: usize, message: impl Into<String>) -> Error {
    let (line, column) = cur.line_col(pos);
    Error { message: message.into(), line, column }
}

pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

fn is_inline_ws(c: char) -> bool {
    c == ' ' || c == '\t'
}

pub(crate) fn skip_inline_ws(cur: &mut Cursor) {
    cur.eat_while(is_inline_ws);
}

pub(crate) fn skip_ws_and_newlines(cur: &mut Cursor) {
    cur.eat_while(|c| is_inline_ws(c) || c == '\n' || c == '\r');
}

pub(crate) fn eat_ident<'a>(cur: &mut Cursor<'a>) -> &'a str {
    cur.eat_while(is_ident_char)
}

/// Raw scalar text stops at any character that could plausibly end an
/// entry/element/sequence item. Not part of `is_ident_char` because scalar
/// values (URLs, file paths) routinely contain `:`, `/`, etc.
fn eat_scalar_raw<'a>(cur: &mut Cursor<'a>) -> &'a str {
    cur.eat_while(|c| !matches!(c, ',' | ')' | ']' | '}' | '\n' | '\r'))
}

fn scalar_from_text(s: &str) -> Value {
    match s {
        "" | "null" => return Value::Null,
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    if let Ok(i) = s.parse::<i64>() {
        return Value::Int(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }
    Value::String(s.to_string())
}

pub(crate) fn parse_quoted(cur: &mut Cursor) -> Result<String> {
    let start = cur.pos();
    if !cur.eat_str("\"") {
        return Err(err(cur, start, "expected '\"'"));
    }
    let mut out = String::new();
    loop {
        match cur.bump() {
            None => return Err(err(cur, cur.pos(), "unterminated string")),
            Some('"') => break,
            Some('\\') => match cur.bump() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => return Err(err(cur, cur.pos(), "unterminated string escape")),
            },
            Some(c) => out.push(c),
        }
    }
    Ok(out)
}

fn parse_seq(cur: &mut Cursor) -> Result<Value> {
    let start = cur.pos();
    if !cur.eat_str("[") {
        return Err(err(cur, start, "expected '['"));
    }
    let mut items = Vec::new();
    loop {
        skip_ws_and_newlines(cur);
        if cur.peek() == Some(']') {
            break;
        }
        items.push(parse_value_at(cur)?);
        skip_ws_and_newlines(cur);
        match cur.peek() {
            Some(',') => {
                cur.bump();
            }
            Some(']') => break,
            _ => return Err(err(cur, cur.pos(), "expected ',' or ']' in sequence")),
        }
    }
    if !cur.eat_str("]") {
        return Err(err(cur, cur.pos(), "expected ']'"));
    }
    Ok(Value::Seq(items))
}

/// A map body is one or more `key: value` entries, separated by commas
/// and/or newlines. Stops at EOF or at a closing bracket it doesn't own
/// (`)`/`}`/`]`) without consuming it -- the caller (an outer `(`/`{`
/// parser, or the top-level `parse_value`) owns that bracket/EOF.
fn parse_map_body(cur: &mut Cursor) -> Result<Value> {
    let mut entries = Vec::new();
    loop {
        skip_ws_and_newlines(cur);
        if matches!(cur.peek(), None | Some(')') | Some('}') | Some(']')) {
            break;
        }
        let key = eat_ident(cur);
        if key.is_empty() {
            return Err(err(cur, cur.pos(), "expected a key"));
        }
        let key = key.to_string();
        skip_inline_ws(cur);
        if !cur.eat_str(":") {
            return Err(err(cur, cur.pos(), "expected ':' after key"));
        }
        skip_inline_ws(cur);
        let value = parse_entry_value(cur)?;
        entries.push((key, value));
        skip_inline_ws(cur);
        match cur.peek() {
            Some(',') => {
                cur.bump();
            }
            Some('\n') | Some('\r') => {}
            other if matches!(other, None | Some(')') | Some('}') | Some(']')) => break,
            _ => return Err(err(cur, cur.pos(), "expected ',' or a newline between entries")),
        }
    }
    Ok(Value::Map(entries))
}

/// The value half of an already-recognized `key: value` entry. Unlike
/// `parse_value_at`, a bare word here is always a plain scalar -- it must
/// NOT be re-checked for a further `word:` map-body pattern, or scalars
/// that themselves contain a colon (URLs, `12:34` timestamps, ...) would
/// be misread as a nested key.
fn parse_entry_value(cur: &mut Cursor) -> Result<Value> {
    skip_ws_and_newlines(cur);
    match cur.peek() {
        Some('[') => parse_seq(cur),
        Some('"') => Ok(Value::String(parse_quoted(cur)?)),
        Some('{') => {
            cur.bump();
            let v = parse_map_body(cur)?;
            skip_ws_and_newlines(cur);
            if !cur.eat_str("}") {
                return Err(err(cur, cur.pos(), "expected '}'"));
            }
            Ok(v)
        }
        _ => {
            let raw = eat_scalar_raw(cur).trim();
            if raw.is_empty() {
                return Err(err(cur, cur.pos(), "expected a value"));
            }
            Ok(scalar_from_text(raw))
        }
    }
}

/// A "fresh" value position: the whole content of `(...)`/top-level `{...}`
/// data, a sequence item, or the entire data-only document. May itself be
/// a map body, so a bare leading word is checked for a following `:`.
pub(crate) fn parse_value_at(cur: &mut Cursor) -> Result<Value> {
    skip_ws_and_newlines(cur);
    match cur.peek() {
        Some('[') => parse_seq(cur),
        Some('"') => Ok(Value::String(parse_quoted(cur)?)),
        Some('{') => {
            cur.bump();
            let v = parse_map_body(cur)?;
            skip_ws_and_newlines(cur);
            if !cur.eat_str("}") {
                return Err(err(cur, cur.pos(), "expected '}'"));
            }
            Ok(v)
        }
        _ => parse_map_body_or_scalar(cur),
    }
}

fn parse_map_body_or_scalar(cur: &mut Cursor) -> Result<Value> {
    let checkpoint = cur.pos();
    let key = eat_ident(cur);
    if key.is_empty() {
        return Err(err(cur, checkpoint, "expected a value"));
    }
    skip_inline_ws(cur);
    let is_map = cur.peek() == Some(':');
    cur.set_pos(checkpoint);
    if is_map {
        parse_map_body(cur)
    } else {
        let raw = eat_scalar_raw(cur).trim();
        Ok(scalar_from_text(raw))
    }
}
