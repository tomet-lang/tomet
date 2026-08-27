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
        items.push(parse_seq_item(cur)?);
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

/// Sentinel key for a positional (unkeyed) entry inside a map body --
/// never producible as a real user-written key (a real key always needs
/// at least one identifier character; `eat_ident` never matches empty).
/// `typedmark-semantics::positional` folds entries under this key into
/// whichever element-specific positional slot they belong to (e.g.
/// `@link`'s `target`, or a `@settings`-defined custom element's own
/// `positional:[...]` list).
pub(crate) const POSITIONAL_ENTRY_KEY: &str = "";

/// One entry inside a map body: an explicit `key: value` (Group A's
/// `identifier:` rule, `://` as the sole exception -- see
/// `is_scheme_uri_colon` -- and a leading `/` as the sole
/// always-bare-scalar exception, see `starts_absolute_path`), or, if
/// neither of those shapes matches, a bare value with no key at all,
/// tagged with [`POSITIONAL_ENTRY_KEY`].
fn parse_one_entry(cur: &mut Cursor) -> Result<(String, Value)> {
    let checkpoint = cur.pos();
    if !starts_absolute_path(cur) {
        let key = eat_ident(cur);
        if !key.is_empty() {
            skip_inline_ws(cur);
            if !is_scheme_uri_colon(cur) && cur.peek() == Some(':') {
                let key = key.to_string();
                cur.bump();
                skip_inline_ws(cur);
                let value = parse_entry_value(cur)?;
                return Ok((key, value));
            }
        }
    }
    cur.set_pos(checkpoint);
    let raw = eat_scalar_raw(cur).trim();
    if raw.is_empty() {
        return Err(err(cur, checkpoint, "expected a value"));
    }
    Ok((POSITIONAL_ENTRY_KEY.to_string(), scalar_from_text(raw)))
}

/// A map body is one or more entries, separated by commas and/or
/// newlines -- each entry either `key: value` or a bare positional value
/// (see [`parse_one_entry`]). Stops at EOF or at a closing bracket it
/// doesn't own (`)`/`}`/`]`) without consuming it -- the caller (an outer
/// `(`/`{` parser, or the top-level `parse_value`) owns that bracket/EOF.
fn parse_map_body(cur: &mut Cursor) -> Result<Value> {
    let mut entries = Vec::new();
    loop {
        skip_ws_newlines_and_comments(cur);
        if matches!(cur.peek(), None | Some(')') | Some('}') | Some(']')) {
            break;
        }
        entries.push(parse_one_entry(cur)?);
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

/// Shared prefix for both group-body and single-item value parsing: the
/// three self-delimiting shapes (`[...]` seq, `"..."` quoted string,
/// `{...}` nested map) are handled identically either way. `fallback`
/// covers everything else, where the two contexts genuinely differ: a
/// group body ([`parse_value_at`]) may collect several comma-separated
/// entries into one `Value::Map`, but a single sequence item
/// ([`parse_seq_item`]) must parse *exactly* one value and leave any
/// following `,` for `parse_seq`'s own loop to see -- otherwise `[a, b]`
/// would collapse into one two-entry map instead of two separate items.
fn parse_value_shape(
    cur: &mut Cursor,
    fallback: impl FnOnce(&mut Cursor) -> Result<Value>,
) -> Result<Value> {
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
        _ => fallback(cur),
    }
}

/// A "fresh" value position: the whole content of `(...)`/top-level
/// `{...}` data, or the entire data-only document -- may collect several
/// comma-separated entries (`key: value` pairs and/or bare positional
/// values) into one `Value::Map`. Not used for sequence items -- see
/// [`parse_seq_item`].
pub(crate) fn parse_value_at(cur: &mut Cursor) -> Result<Value> {
    parse_value_shape(cur, parse_map_body_or_scalar)
}

/// A single item inside `[...]`: exactly one value (a `key: value` pair
/// wrapped in a one-entry map, or a bare positional value returned
/// as-is), never consuming a following `,` -- that belongs to
/// `parse_seq`'s own loop, not this function.
fn parse_seq_item(cur: &mut Cursor) -> Result<Value> {
    parse_value_shape(cur, |cur| {
        let (key, value) = parse_one_entry(cur)?;
        if key == POSITIONAL_ENTRY_KEY {
            Ok(value)
        } else {
            Ok(Value::Map(vec![(key, value)]))
        }
    })
}

/// A leading `/` can never start a map key (`is_ident_char` excludes it),
/// so a value here is unambiguously a bare scalar -- an OS-absolute path
/// like `/readme.md`. Skipped straight to `eat_scalar_raw` rather than
/// falling through `eat_ident` (which would just read an empty key and
/// error).
fn starts_absolute_path(cur: &Cursor) -> bool {
    cur.peek() == Some('/')
}

/// Whether `cur`, positioned right after an identifier and its trailing
/// inline whitespace, sits at a `://` -- i.e. the identifier just read is
/// a URI scheme (`https`, `file`, ...) and the whole `scheme://...` is one
/// bare external-URL scalar, not a `key: value` map entry. No real map
/// value can start with a literal `//` (per `skip_ws_newlines_and_comments`'s
/// doc comment: an unquoted leading `//` is always read as a comment, so a
/// `key: //...` entry can never carry an actual value), so this check
/// can't misfire on a legitimate map whose value happens to start that
/// way.
fn is_scheme_uri_colon(cur: &Cursor) -> bool {
    if cur.peek() != Some(':') {
        return false;
    }
    let mut look = *cur;
    look.bump();
    look.starts_with("//")
}

/// A "fresh" value position that might be a single bare scalar, one
/// `key: value` pair, several `key: value` pairs, or several bare
/// positional values with no keys at all (`(a, b)` -- previously a parse
/// error, now valid; see [`parse_one_entry`]). Parses the same entry loop
/// [`parse_map_body`] does; if that produced exactly one entry and it's
/// tagged with [`POSITIONAL_ENTRY_KEY`] (no real key), collapses to that
/// entry's bare value instead of wrapping it in a one-entry `Value::Map`
/// -- this is what keeps every existing single-scalar case
/// (`https://example.com`, `/etc/hosts`, `hello`, ...) parsing to exactly
/// the same `Value` as before this generalization.
fn parse_map_body_or_scalar(cur: &mut Cursor) -> Result<Value> {
    let checkpoint = cur.pos();
    skip_ws_newlines_and_comments(cur);
    if matches!(cur.peek(), None | Some(')') | Some('}') | Some(']')) {
        return Err(err(cur, checkpoint, "expected a value"));
    }
    cur.set_pos(checkpoint);
    let value = parse_map_body(cur)?;
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
