//! Parsing for `Value`: map bodies (`key: value`), `[..]` sequences,
//! `"..."` strings, and bare scalars. Shared by the data-only entry point
//! (`parse_value`) and by `document.rs` for `(args)` groups and
//! data-shaped `{value}` groups.

use crate::error::{Error, Result};
use typedmark_ast::Value;
use typedmark_lexar::Cursor;

/// Parse an entire source string as one `Value` (a data-only `.tm`
/// document -- no headings, prose, or elements).
pub fn parse_value(src: &str) -> Result<Value> {
    let mut cur = Cursor::new(src);
    let value = parse_value_at(&mut cur)?;
    skip_ws_newlines_and_comments(&mut cur);
    if !cur.is_eof() {
        return Err(err(&cur, cur.pos(), "unexpected trailing content"));
    }
    Ok(value)
}

pub(crate) fn err(cur: &Cursor, pos: usize, message: impl Into<String>) -> Error {
    let (line, column) = cur.line_col(pos);
    Error {
        message: message.into(),
        line,
        column,
        offset: pos,
    }
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

/// `// ...` to end of line/EOF -- the value-grammar sibling of
/// `document.rs`'s block-level `skip_line_comment`.
pub(crate) fn skip_line_comment(cur: &mut Cursor) {
    cur.eat_str("//");
    cur.eat_while(|c| c != '\n' && c != '\r');
}

pub(crate) fn skip_block_comment(cur: &mut Cursor) -> Result<()> {
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
/// Safe to use anywhere within `(...)`/`{...}`/`[...]` because it only
/// ever runs at a "gap" position -- the start of a fresh value, between
/// map entries, between sequence items -- never mid-scalar, so it can't
/// misfire on a bare scalar that happens to contain `//` (a URL like
/// `https://example.com` is consumed as one token by `eat_scalar_raw`
/// before this could run again). A bare scalar that must start with a
/// literal `//` (e.g. a protocol-relative URL) needs to be quoted, same
/// trade-off as other special characters in bare scalars.
pub(crate) fn skip_ws_newlines_and_comments(cur: &mut Cursor) {
    loop {
        skip_ws_and_newlines(cur);
        if cur.starts_with("//") {
            skip_line_comment(cur);
            continue;
        }
        break;
    }
}

pub(crate) fn eat_ident<'a>(cur: &mut Cursor<'a>) -> &'a str {
    cur.eat_while(is_ident_char)
}

/// Advances `cur` to the position of the `close` that matches the `open`
/// already consumed just before `cur`'s current position, tracking nested
/// `open`/`close` pairs and skipping over `"`/`'`-quoted runs (so a quoted
/// `close`/`open` character -- e.g. a `"}"` inside a JSON string, or a
/// `"]"` inside a codeblock's string literal -- can't miscount). Doesn't
/// consume the closing delimiter. Used where a group's body is real
/// source in some *other* language (embedded JSON/YAML/TOML, or a
/// codeblock's source code) and only the matching bracket/brace needs
/// finding -- quote-awareness matters there because unmatched quotes
/// don't happen in valid source. For a body that's free-form prose
/// instead (no language guarantees balanced quotes -- an apostrophe like
/// `don't` would otherwise be misread as opening a quoted run and swallow
/// the rest of the text), see the quote-agnostic
/// [`find_matching_bracket`].
pub(crate) fn find_matching_delimiter(
    cur: &mut Cursor,
    open: char,
    close: char,
    group_start: usize,
) -> Result<usize> {
    let mut depth: u32 = 0;
    loop {
        match cur.peek() {
            None => {
                return Err(err(
                    cur,
                    group_start,
                    format!("unterminated '{open}', expected matching '{close}'"),
                ));
            }
            Some('"') => skip_quoted(cur, '"'),
            Some('\'') => skip_quoted(cur, '\''),
            Some(c) if c == open => {
                depth += 1;
                cur.bump();
            }
            Some(c) if c == close => {
                if depth == 0 {
                    return Ok(cur.pos());
                }
                depth -= 1;
                cur.bump();
            }
            Some(_) => {
                cur.bump();
            }
        }
    }
}

/// Same contract as [`find_matching_delimiter`] (nested `open`/`close`
/// depth, doesn't consume the closing delimiter), but deliberately does
/// *not* skip quoted runs -- a free-form-prose raw body (e.g. `content:raw`)
/// has no guarantee its `"`/`'` occurrences are balanced the way real
/// source code's are, so quote-skipping there would misfire on an
/// ordinary apostrophe.
pub(crate) fn find_matching_bracket(
    cur: &mut Cursor,
    open: char,
    close: char,
    group_start: usize,
) -> Result<usize> {
    let mut depth: u32 = 0;
    loop {
        match cur.peek() {
            None => {
                return Err(err(
                    cur,
                    group_start,
                    format!("unterminated '{open}', expected matching '{close}'"),
                ));
            }
            Some(c) if c == open => {
                depth += 1;
                cur.bump();
            }
            Some(c) if c == close => {
                if depth == 0 {
                    return Ok(cur.pos());
                }
                depth -= 1;
                cur.bump();
            }
            Some(_) => {
                cur.bump();
            }
        }
    }
}

/// Skips a `quote`-delimited run starting at the opening quote. Backslash
/// escapes are only honored for `"` (JSON/TOML basic strings, and the
/// common convention in most C-like source) -- `'` strings (TOML literal
/// strings, YAML single-quoted scalars, and many languages' char literals)
/// have no backslash escaping in either format, so `\` there is just a
/// literal character. Runs to EOF harmlessly if unterminated; the caller's
/// own EOF check reports that as "unterminated '<open>'" once the outer
/// loop sees it.
pub(crate) fn skip_quoted(cur: &mut Cursor, quote: char) {
    cur.bump();
    loop {
        match cur.peek() {
            None => break,
            Some('\\') if quote == '"' => {
                cur.bump();
                cur.bump();
            }
            Some(c) => {
                cur.bump();
                if c == quote {
                    break;
                }
            }
        }
    }
}

/// Raw scalar text stops at any character that could plausibly end an
/// entry/element/sequence item. Not part of `is_ident_char` because scalar
/// values (URLs, file paths) routinely contain `:`, `/`, etc. Also stops
/// early at a `//` immediately preceded by whitespace -- the start of a
/// trailing comment -- leaving it for the caller's subsequent gap-skip to
/// consume. A `//` with no whitespace before it (`https://example.com`)
/// stays literal: same boundary rule `is_boundary` already uses for
/// `*em*`/`_em_` delimiters, just applied to comments.
fn eat_scalar_raw<'a>(cur: &mut Cursor<'a>) -> &'a str {
    let start = cur.pos();
    loop {
        match cur.peek() {
            None => break,
            Some(c) if matches!(c, ',' | ')' | ']' | '}' | '\n' | '\r') => break,
            Some(c) if is_inline_ws(c) => {
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
        skip_ws_newlines_and_comments(cur);
        if cur.peek() == Some(']') {
            break;
        }
        items.push(parse_value_at(cur)?);
        skip_ws_newlines_and_comments(cur);
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
        skip_ws_newlines_and_comments(cur);
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
        // A trailing `// comment` on the same line as the value: `eat_scalar_raw`
        // already stopped short of it (boundary rule), so it's still here to
        // consume before checking for the `,`/newline/close that separates entries.
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

/// The value half of an already-recognized `key: value` entry. Unlike
/// `parse_value_at`, a bare word here is always a plain scalar -- it must
/// NOT be re-checked for a further `word:` map-body pattern, or scalars
/// that themselves contain a colon (URLs, `12:34` timestamps, ...) would
/// be misread as a nested key.
fn parse_entry_value(cur: &mut Cursor) -> Result<Value> {
    skip_ws_newlines_and_comments(cur);
    match cur.peek() {
        Some('[') => parse_seq(cur),
        Some('"') => Ok(Value::String(parse_quoted(cur)?)),
        Some('{') => {
            cur.bump();
            let v = parse_map_body(cur)?;
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

/// A "fresh" value position: the whole content of `(...)`/top-level `{...}`
/// data, a sequence item, or the entire data-only document. May itself be
/// a map body, so a bare leading word is checked for a following `:`.
pub(crate) fn parse_value_at(cur: &mut Cursor) -> Result<Value> {
    skip_ws_newlines_and_comments(cur);
    match cur.peek() {
        Some('[') => parse_seq(cur),
        Some('"') => Ok(Value::String(parse_quoted(cur)?)),
        Some('{') => {
            cur.bump();
            let v = parse_map_body(cur)?;
            skip_ws_newlines_and_comments(cur);
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
