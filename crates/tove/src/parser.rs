//! Parsing for TOVE (Tomet Object & Value Expression):
//! map bodies (`key: value`), `{ ... }` blocks, `"..."` strings,
//! `call(...)` expressions, and bare scalars.
//!
//! A list value is spelled `list(...)` (a `Value::Call`,
//! normalized to `Value::Seq` downstream).

use crate::error::{Error, Result};
use tomet_ast::{Name, Value};
use tomet_lexer::Cursor;

/// A hook allowing callers (such as `tomet-syntax-parser`) to parse custom
/// embedded values (e.g. `@element` or `$interp`) at value positions.
pub trait ValueHook {
    fn try_parse_value(&mut self, cur: &mut Cursor) -> Option<Result<Value>>;
}

/// The default no-op hook for pure TOVE documents.
pub struct NoHook;

impl ValueHook for NoHook {
    fn try_parse_value(&mut self, _cur: &mut Cursor) -> Option<Result<Value>> {
        None
    }
}

impl<F> ValueHook for F
where
    F: FnMut(&mut Cursor) -> Option<Result<Value>>,
{
    fn try_parse_value(&mut self, cur: &mut Cursor) -> Option<Result<Value>> {
        self(cur)
    }
}

/// Parse an entire source string as one `Value` using pure TOVE syntax.
pub fn parse_value(src: &str) -> Result<Value> {
    parse_value_with(src, &mut NoHook)
}

/// Parse an entire source string as one `Value` with a custom [`ValueHook`].
pub fn parse_value_with<H: ValueHook>(src: &str, hook: &mut H) -> Result<Value> {
    let mut cur = Cursor::new(src);
    let value = parse_value_at_with(&mut cur, hook)?;
    skip_ws_newlines_and_comments(&mut cur);
    if !cur.is_eof() {
        return Err(err(&cur, cur.pos(), "unexpected trailing content"));
    }
    Ok(value)
}

pub fn err(cur: &Cursor, pos: usize, message: impl Into<String>) -> Error {
    let (line, column) = cur.line_col(pos);
    Error::at(message, line, column, pos)
}

/// Characters legal inside a **map key**.
pub fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// First character of one segment of an element/identifier name.
pub fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Subsequent characters of one segment of an element/identifier name.
pub fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

pub fn is_inline_ws(c: char) -> bool {
    c == ' ' || c == '\t'
}

pub fn skip_inline_ws(cur: &mut Cursor) {
    cur.eat_while(is_inline_ws);
}

pub fn skip_ws_and_newlines(cur: &mut Cursor) {
    cur.eat_while(|c| is_inline_ws(c) || c == '\n' || c == '\r');
}

/// `// ...` to end of line/EOF.
pub fn skip_line_comment(cur: &mut Cursor) {
    cur.eat_str("//");
    cur.eat_while(|c| c != '\n' && c != '\r');
}

pub fn skip_block_comment(cur: &mut Cursor) -> Result<()> {
    let group_start = cur.pos();
    cur.eat_str("/*");
    loop {
        if cur.is_eof() {
            return Err(err(
                cur,
                group_start,
                "unterminated block comment '/*', expected '*/'",
            ));
        }
        if cur.starts_with("*/") {
            cur.eat_str("*/");
            break;
        }
        cur.bump();
    }
    Ok(())
}

/// Like `skip_ws_and_newlines`, but also consumes `//` line comments.
pub fn skip_ws_newlines_and_comments(cur: &mut Cursor) {
    loop {
        skip_ws_and_newlines(cur);
        if cur.starts_with("//") {
            skip_line_comment(cur);
            continue;
        }
        break;
    }
}

pub fn eat_ident<'a>(cur: &mut Cursor<'a>) -> &'a str {
    cur.eat_while(is_ident_char)
}

/// Reads one `.`-separated name, or `None` if `cur` is not at a name start.
pub fn eat_name(cur: &mut Cursor) -> Option<Name> {
    let first = eat_name_segment(cur)?;
    let mut segments = vec![first];
    loop {
        let checkpoint = cur.pos();
        if cur.peek() != Some('.') {
            break;
        }
        cur.bump();
        match eat_name_segment(cur) {
            Some(segment) => segments.push(segment),
            None => {
                cur.set_pos(checkpoint);
                break;
            }
        }
    }
    let name = segments.pop().expect("at least one segment").to_string();
    let namespace = if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    };
    Some(Name { namespace, name })
}

pub fn eat_name_segment<'a>(cur: &mut Cursor<'a>) -> Option<&'a str> {
    if !cur.peek().is_some_and(is_name_start) {
        return None;
    }
    Some(cur.eat_while(is_name_char))
}

/// Whether `cur` is at the start of a name, without consuming it.
pub fn is_name_start_at(cur: &Cursor) -> bool {
    cur.peek().is_some_and(is_name_start)
}

/// Raw scalar text stops at any character that could plausibly end an
/// entry/sequence item.
pub fn eat_scalar_raw<'a>(cur: &mut Cursor<'a>) -> &'a str {
    let start = cur.pos();
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    loop {
        match cur.peek() {
            None => break,
            Some('"') => {
                cur.bump();
                while let Some(c) = cur.bump() {
                    if c == '"' {
                        break;
                    }
                    if c == '\\' {
                        cur.bump();
                    }
                }
            }
            Some('(') => {
                paren_depth += 1;
                cur.bump();
            }
            Some(')') if paren_depth > 0 => {
                paren_depth -= 1;
                cur.bump();
            }
            Some('[') => {
                bracket_depth += 1;
                cur.bump();
            }
            Some(']') if bracket_depth > 0 => {
                bracket_depth -= 1;
                cur.bump();
            }
            Some('{') => {
                brace_depth += 1;
                cur.bump();
            }
            Some('}') if brace_depth > 0 => {
                brace_depth -= 1;
                cur.bump();
            }
            Some(c)
                if matches!(c, ',' | ')' | ']' | '}' | '\n' | '\r')
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0 =>
            {
                break;
            }
            Some(c)
                if is_inline_ws(c)
                    && paren_depth == 0
                    && bracket_depth == 0
                    && brace_depth == 0 =>
            {
                let mut look = *cur;
                look.eat_while(is_inline_ws);
                if look.starts_with("//") {
                    break;
                }
                cur.bump();
            }
            Some(_) => {
                cur.bump();
            }
        }
    }
    &cur.src()[start..cur.pos()]
}

/// `name(arg, arg, ...)` -- an immediately-resolved literal such as
/// `list(card, ns.mycard)`.
pub fn try_parse_call(cur: &mut Cursor) -> Option<Result<Value>> {
    try_parse_call_with(cur, &mut NoHook)
}

pub fn try_parse_call_with<H: ValueHook>(cur: &mut Cursor, hook: &mut H) -> Option<Result<Value>> {
    let mut look = *cur;
    let name = eat_name_segment(&mut look)?.to_string();
    if look.peek() != Some('(') {
        return None;
    }
    *cur = look;
    cur.bump();
    let mut args = Vec::new();
    loop {
        skip_ws_newlines_and_comments(cur);
        if cur.peek() == Some(')') {
            break;
        }
        match parse_entry_value_with(cur, hook) {
            Ok(v) => args.push(v),
            Err(e) => return Some(Err(e)),
        }
        skip_ws_newlines_and_comments(cur);
        match cur.peek() {
            Some(',') => {
                cur.bump();
            }
            Some(')') => break,
            _ => {
                return Some(Err(err(
                    cur,
                    cur.pos(),
                    "expected ',' or ')' in call arguments",
                )));
            }
        }
    }
    if !cur.eat_str(")") {
        return Some(Err(err(cur, cur.pos(), "expected ')'")));
    }
    Some(Ok(Value::Call(name, args)))
}

pub fn scalar_from_text(s: &str) -> Value {
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

pub fn parse_quoted(cur: &mut Cursor) -> Result<String> {
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

/// Sentinel key for a positional (unkeyed) entry inside a map body.
pub const POSITIONAL_ENTRY_KEY: &str = "";

pub fn parse_one_entry(cur: &mut Cursor) -> Result<(String, Value)> {
    parse_one_entry_with(cur, &mut NoHook)
}

pub fn parse_one_entry_with<H: ValueHook>(
    cur: &mut Cursor,
    hook: &mut H,
) -> Result<(String, Value)> {
    let checkpoint = cur.pos();
    if !starts_absolute_path(cur) {
        let key = eat_ident(cur);
        if !key.is_empty() {
            skip_inline_ws(cur);
            if !is_scheme_uri_colon(cur) && cur.peek() == Some(':') {
                let key = key.to_string();
                cur.bump();
                skip_inline_ws(cur);
                let value = parse_entry_value_with(cur, hook)?;
                return Ok((key, value));
            }
        }
    }
    cur.set_pos(checkpoint);
    let value = parse_entry_value_with(cur, hook)?;
    Ok((POSITIONAL_ENTRY_KEY.to_string(), value))
}

pub fn parse_map_body(cur: &mut Cursor) -> Result<Value> {
    parse_map_body_with(cur, &mut NoHook)
}

pub fn parse_map_body_with<H: ValueHook>(cur: &mut Cursor, hook: &mut H) -> Result<Value> {
    let mut entries = Vec::new();
    loop {
        skip_ws_newlines_and_comments(cur);
        if matches!(cur.peek(), None | Some(')') | Some('}') | Some(']')) {
            break;
        }
        entries.push(parse_one_entry_with(cur, hook)?);
        skip_inline_ws(cur);
        if cur.starts_with("//") {
            skip_line_comment(cur);
        }
        match cur.peek() {
            Some(',') => {
                cur.bump();
            }
            Some('\n') | Some('\r') => {}
            other if matches!(other, None | Some(')') | Some('}') | Some(']')) => break,
            _ => {
                return Err(err(
                    cur,
                    cur.pos(),
                    "expected ',' or a newline between entries",
                ));
            }
        }
    }
    Ok(Value::Map(entries))
}

pub fn parse_entry_value(cur: &mut Cursor) -> Result<Value> {
    parse_entry_value_with(cur, &mut NoHook)
}

pub fn parse_entry_value_with<H: ValueHook>(cur: &mut Cursor, hook: &mut H) -> Result<Value> {
    skip_ws_newlines_and_comments(cur);
    if let Some(result) = hook.try_parse_value(cur) {
        return result;
    }
    if let Some(result) = try_parse_call_with(cur, hook) {
        return result;
    }
    match cur.peek() {
        Some('[') => Err(err(
            cur,
            cur.pos(),
            "'[...]' list literal was removed -- write 'list(...)' instead",
        )),
        Some('"') => Ok(Value::String(parse_quoted(cur)?)),
        Some('{') => {
            cur.bump();
            let v = parse_map_body_with(cur, hook)?;
            skip_ws_newlines_and_comments(cur);
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

pub fn parse_value_at(cur: &mut Cursor) -> Result<Value> {
    parse_value_at_with(cur, &mut NoHook)
}

pub fn parse_value_at_with<H: ValueHook>(cur: &mut Cursor, hook: &mut H) -> Result<Value> {
    parse_map_body_or_scalar(cur, hook)
}

pub fn starts_absolute_path(cur: &Cursor) -> bool {
    cur.peek() == Some('/')
}

pub fn is_scheme_uri_colon(cur: &Cursor) -> bool {
    if cur.peek() != Some(':') {
        return false;
    }
    let mut look = *cur;
    look.bump();
    look.starts_with("//")
}

fn parse_map_body_or_scalar<H: ValueHook>(cur: &mut Cursor, hook: &mut H) -> Result<Value> {
    let checkpoint = cur.pos();
    skip_ws_newlines_and_comments(cur);
    if matches!(cur.peek(), None | Some(')') | Some('}') | Some(']')) {
        return Err(err(cur, checkpoint, "expected a value"));
    }
    cur.set_pos(checkpoint);
    let value = parse_map_body_with(cur, hook)?;
    let Value::Map(entries) = &value else {
        unreachable!("parse_map_body always returns Value::Map")
    };
    if let [(key, v)] = &entries[..]
        && key == POSITIONAL_ENTRY_KEY
    {
        return Ok(v.clone());
    }
    Ok(value)
}
