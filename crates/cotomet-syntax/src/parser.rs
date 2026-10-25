//! Parsing for cotomet data:
//! map bodies (`key: value`), `{ ... }` blocks, `"..."` strings,
//! `call(...)` expressions, and bare scalars.
//!
//! A list value is spelled `list(...)` (a `Value::Call`,
//! normalized to `Value::Seq` downstream).

use std::collections::HashSet;

use crate::error::{Error, Result};
use tomet_ast::{Name, Value};
use tomet_lexer::Cursor;

/// A hook allowing callers (such as `tomet-syntax-parser`) to parse custom
/// embedded values (e.g. `@element` or `$interp`) at value positions.
pub trait ValueHook {
    fn try_parse_value(&mut self, cur: &mut Cursor) -> Option<Result<Value>>;
}

/// The default no-op hook for pure cotomet documents.
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

/// Parse an entire source string as one `Value` using pure cotomet syntax.
///
/// A source with nothing but whitespace and comments is the empty map.
pub fn parse_value(src: &str) -> Result<Value> {
    parse_value_with(src, &mut NoHook)
}

/// Parse an entire source string as one `Value` with a custom [`ValueHook`].
///
/// A source with nothing but whitespace and comments is the empty map. Only
/// the document may be empty: a value slot inside it (`k:`) still needs a value.
pub fn parse_value_with<H: ValueHook>(src: &str, hook: &mut H) -> Result<Value> {
    let mut cur = Cursor::new(src);
    skip_ws_newlines_and_comments(&mut cur);
    if cur.is_eof() {
        return Ok(Value::Map(Vec::new()));
    }
    cur.set_pos(0);
    let value = parse_value_at_with(&mut cur, hook).map_err(|e| e.located_in(src))?;
    skip_ws_newlines_and_comments(&mut cur);
    if !cur.is_eof() {
        return Err(err(cur.pos(), "unexpected trailing content").located_in(src));
    }
    Ok(value)
}

pub fn err(pos: usize, message: impl Into<String>) -> Error {
    Error::at_offset(message, pos)
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

/// The arguments of a call, the `(` already read: values separated by `,`,
/// through the closing `)`.
fn parse_call_args<H: ValueHook>(
    cur: &mut Cursor,
    hook: &mut H,
    raw_args: bool,
) -> Result<Vec<Value>> {
    let mut args = Vec::new();
    loop {
        skip_ws_newlines_and_comments(cur);
        if cur.peek() == Some(')') {
            break;
        }
        args.push(parse_entry_value_impl(cur, hook, raw_args)?);
        skip_ws_newlines_and_comments(cur);
        match cur.peek() {
            Some(',') => {
                cur.bump();
            }
            Some(')') => break,
            _ => return Err(err(cur.pos(), "expected ',' or ')' in call arguments")),
        }
    }
    if !cur.eat_str(")") {
        return Err(err(cur.pos(), "expected ')'"));
    }
    Ok(args)
}

/// The callees the parser resolves to a plain [`Value`] at parse time: the
/// literal constructors. Any other `name(...)` stays a [`Value::Call`] for a
/// later pass to judge (`enum(...)` among them, which has no meaning yet).
///
/// - `list(a, b)` is a [`Value::Seq`].
/// - `string(a)` is the text of `a` ([`construct_string`]); a bare `a` is
///   taken as written, so `string(1.50)` is `"1.50"`.
/// - `int(x)` and `float(x)` convert like Python's ([`construct_int`],
///   [`construct_float`]).
pub const LITERAL_CONSTRUCTORS: &[&str] = &["list", "string", "int", "float"];

pub fn try_parse_call_with<H: ValueHook>(cur: &mut Cursor, hook: &mut H) -> Option<Result<Value>> {
    let mut look = *cur;
    let call_start = cur.pos();
    let name = eat_name_segment(&mut look)?.to_string();
    if look.peek() != Some('(') {
        return None;
    }
    *cur = look;
    cur.bump();
    // `string(a)` is the text `a`, so its bare arguments are not typed.
    let raw_args = name == "string";
    let args = match parse_call_args(cur, hook, raw_args) {
        Ok(args) => args,
        Err(e) => return Some(Err(e)),
    };
    let resolved = match name.as_str() {
        "list" => Ok(Value::Seq(args)),
        "string" => construct_string(args),
        "int" => construct_int(args),
        "float" => construct_float(args),
        _ => return Some(Ok(Value::Call(name, args))),
    };
    Some(resolved.map_err(|message| err(call_start, message)))
}

/// The one argument of a constructor that takes at most one, `None` for no
/// argument; more than one is an error.
fn at_most_one(name: &str, args: Vec<Value>) -> std::result::Result<Option<Value>, String> {
    let mut args = args.into_iter();
    let arg = args.next();
    if args.next().is_some() {
        return Err(format!("{name}() takes at most one argument"));
    }
    Ok(arg)
}

/// `string(x)`: the text of `x`. `string()` is `""`. A bare argument arrives
/// as its written text (see [`parse_entry_value_impl`]), so `string(a)` is
/// `"a"` and `string(1.50)` is `"1.50"`; a quoted string is itself; the value
/// of a nested constructor (`string(int(5))`) is rendered the way it is
/// written. A list, a map, or any other call has no text and is an error.
fn construct_string(args: Vec<Value>) -> std::result::Result<Value, String> {
    Ok(Value::String(match at_most_one("string", args)? {
        None => String::new(),
        Some(Value::String(s)) => s,
        Some(Value::Int(i)) => i.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::Number(text)) => text,
        Some(Value::Float(f)) if f.is_finite() => Value::float_text(f),
        Some(Value::Float(f)) if f.is_nan() => "nan".to_string(),
        Some(Value::Float(f)) => (if f > 0.0 { "inf" } else { "-inf" }).to_string(),
        Some(other) => return Err(format!("string() takes a scalar, got {other:?}")),
    }))
}

/// `int(x)`: `int()` is `0`; an int is itself; a float truncates toward zero
/// (and must fit an `i64`); a string is parsed as a decimal `i64`, so `"1.5"`
/// and `"0x10"` are errors. A bool or anything else is an error.
fn construct_int(args: Vec<Value>) -> std::result::Result<Value, String> {
    match at_most_one("int", args)? {
        None => Ok(Value::Int(0)),
        Some(Value::Int(i)) => Ok(Value::Int(i)),
        Some(Value::Float(f)) => {
            // `i64::MAX as f64` rounds up to 2^63, so the upper bound is exclusive.
            if f.is_finite() && f >= i64::MIN as f64 && f < i64::MAX as f64 {
                Ok(Value::Int(f.trunc() as i64))
            } else {
                Err(format!("float `{f}` is out of range for a 64-bit integer"))
            }
        }
        Some(Value::Number(text)) => match text.parse::<f64>() {
            Ok(f) if f.is_finite() && f >= i64::MIN as f64 && f < i64::MAX as f64 => {
                Ok(Value::Int(f.trunc() as i64))
            }
            _ => Err(format!(
                "number `{text}` is out of range for a 64-bit integer"
            )),
        },
        Some(Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| format!("int() cannot convert `{}` to an integer", s.trim())),
        Some(other) => Err(format!("int() takes a number or a string, got {other:?}")),
    }
}

/// `float(x)`: the literal constructor for a float, the only way to spell
/// `nan` and the infinities (which are plain strings as bare scalars).
///
/// `float()` is `0.0`; an int widens; a string is parsed the way Rust and
/// Python parse one (`float(inf)`, `float(-inf)`, `float(nan)`, `float(1.5)`).
/// Anything else, or more than one argument, is an error, as is a finite
/// spelling that overflows (`float(1e999)`).
fn construct_float(args: Vec<Value>) -> std::result::Result<Value, String> {
    match at_most_one("float", args)? {
        None => Ok(Value::Float(0.0)),
        Some(Value::Int(i)) => Ok(Value::Float(i as f64)),
        Some(Value::Float(f)) => Ok(Value::Float(f)),
        Some(Value::Number(text)) => match text.parse::<f64>() {
            Ok(f) if f.is_finite() => Ok(Value::Float(f)),
            _ => Err(format!("float `{text}` is out of range")),
        },
        Some(Value::String(s)) => {
            let text = s.trim();
            let spells_non_finite = text
                .trim_start_matches(['+', '-'])
                .starts_with(|c: char| c.is_ascii_alphabetic());
            match text.parse::<f64>() {
                Ok(f) if f.is_finite() || spells_non_finite => Ok(Value::Float(f)),
                Ok(_) => Err(format!("float `{text}` is out of range")),
                Err(_) => Err(format!("float() cannot convert `{text}` to a float")),
            }
        }
        Some(other) => Err(format!("float() takes a number or a string, got {other:?}")),
    }
}

/// What a bare scalar's text is, by the JSON number grammar.
enum NumberShape {
    Int,
    Float,
}

/// Classifies `s` as `-?(0|[1-9][0-9]*)` (int), the same followed by a
/// fraction (`.digits`) and/or an exponent (`e[+-]digits`) (float), or
/// neither. A leading zero (`007`), a leading `+`, and a bare `.5` or `5.`
/// are none of these, so they stay strings.
fn classify_number(s: &str) -> Option<NumberShape> {
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        (i > start).then_some(i)
    };
    let mut i = usize::from(b.first() == Some(&b'-'));
    match b.get(i)? {
        b'0' => i += 1,
        b'1'..=b'9' => i = digits(i)?,
        _ => return None,
    }
    let mut float = false;
    if b.get(i) == Some(&b'.') {
        i = digits(i + 1)?;
        float = true;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        i = digits(i)?;
        float = true;
    }
    (i == b.len()).then_some(if float {
        NumberShape::Float
    } else {
        NumberShape::Int
    })
}

/// Types the text of a bare scalar (see the crate-level "Scalars" section).
///
/// `null`, `true` and `false` are the only keywords. A number by the JSON
/// grammar is an `i64` or an `f64` when that is exactly what it says, and
/// otherwise a [`Value::Number`] holding the text as written (`1.10`, `1e3`,
/// `-0`, an integer too large for an `i64`, a float too large for an `f64`).
/// Everything else is a string, so no text is ever changed by reading it.
///
/// The `Result` is kept for the callers that match on one; reading a bare scalar
/// has no error left in it.
pub fn scalar_from_text(s: &str) -> std::result::Result<Value, String> {
    match s {
        "null" => return Ok(Value::Null),
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    match classify_number(s) {
        Some(NumberShape::Int) => Ok(match s.parse::<i64>() {
            Ok(i) if i.to_string() == s => Value::Int(i),
            // `-0`, or too large for an `i64`: kept as written.
            _ => Value::Number(s.to_string()),
        }),
        Some(NumberShape::Float) => Ok(match s.parse::<f64>() {
            Ok(f) if f.is_finite() && Value::float_text(f) == s => Value::Float(f),
            // `1.10`, `1e3`, `0.50`, or beyond an `f64`: kept as written.
            _ => Value::Number(s.to_string()),
        }),
        None => Ok(Value::String(s.to_string())),
    }
}

/// Reads a `"..."` string, with `\"`, `\\`, `\n` and `\t` escapes; a raw newline
/// is part of the text. A string that never closes is an error at its opening
/// quote, not at the end of the file where it ran out.
pub fn parse_quoted(cur: &mut Cursor) -> Result<String> {
    let start = cur.pos();
    if !cur.eat_str("\"") {
        return Err(err(start, "expected '\"'"));
    }
    let mut out = String::new();
    loop {
        match cur.bump() {
            None => return Err(err(start, "unterminated string")),
            Some('"') => break,
            Some('\\') => match cur.bump() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => return Err(err(start, "unterminated string")),
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

/// Parses one `key: value` entry, or a bare value as a positional entry.
///
/// A key is a bare identifier (`is_ident_char`) or a quoted string
/// (`"tmtroot/a/b.tmt": value`), which lets a key be a path or a URL. A quoted
/// string with no `:` after it is a positional value, not a key. An empty
/// quoted key is an error: `""` is [`POSITIONAL_ENTRY_KEY`].
pub fn parse_one_entry_with<H: ValueHook>(
    cur: &mut Cursor,
    hook: &mut H,
) -> Result<(String, Value)> {
    let checkpoint = cur.pos();
    if let Some(key) = eat_key(cur) {
        skip_inline_ws(cur);
        if !is_scheme_uri_colon(cur) && cur.peek() == Some(':') {
            if key.is_empty() {
                return Err(err(checkpoint, "a quoted key must not be empty"));
            }
            cur.bump();
            skip_inline_ws(cur);
            let value = parse_entry_value_with(cur, hook)?;
            return Ok((key, value));
        }
    }
    cur.set_pos(checkpoint);
    let value = parse_entry_value_with(cur, hook)?;
    Ok((POSITIONAL_ENTRY_KEY.to_string(), value))
}

/// Reads the text that may be a key: a quoted string, or a non-empty
/// identifier that is not an absolute path. Whether it is a key is decided by
/// the `:` after it, so the caller restores the cursor when it is not.
fn eat_key(cur: &mut Cursor) -> Option<String> {
    if cur.peek() == Some('"') {
        // An unterminated string is no key; the positional parse reports it.
        return parse_quoted(cur).ok();
    }
    if starts_absolute_path(cur) {
        return None;
    }
    let key = eat_ident(cur);
    (!key.is_empty()).then(|| key.to_string())
}

pub fn parse_map_body(cur: &mut Cursor) -> Result<Value> {
    parse_map_body_with(cur, &mut NoHook)
}

/// A map with more entries than this tracks its keys in a set rather than
/// scanning them for each new one.
const KEY_SCAN_LIMIT: usize = 16;

/// The error for a named key written twice in one map. Positional entries
/// (`POSITIONAL_ENTRY_KEY`) are not keys and may repeat.
pub fn duplicate_key_error(pos: usize, key: &str) -> Error {
    err(pos, format!("duplicate key `{key}`"))
}

/// Parses `key: value` entries until a closer or the end, as a `Value::Map`.
///
/// A named key may appear once: a second one is an error at its position
/// ([`duplicate_key_error`]), compared after unquoting, so `"a": 1, a: 2` is
/// a duplicate too. A map that held the same key twice would mean different
/// things to a reader that keeps the first, the last, or both.
pub fn parse_map_body_with<H: ValueHook>(cur: &mut Cursor, hook: &mut H) -> Result<Value> {
    parse_map_body_impl(cur, hook, false)
}

/// A declaration about a file, written `::name(args){data}` at its top: what
/// the file is (`::kind(catalog, 2)`), what it brings in (`::use(deck)`), the
/// information about it (`::meta{ id: x }`).
///
/// The arguments are values, positional, as in any cotomet call; the data is a
/// map. What a name means is not for this crate to say: it reads the shape.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub name: String,
    pub args: Vec<Value>,
    pub data: Option<Value>,
    /// Where the `::` is, in bytes. Not part of what the declaration says, so
    /// two files that say the same thing are equal wherever they say it.
    pub offset: usize,
}

impl PartialEq for Declaration {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.args == other.args && self.data == other.data
    }
}

/// A standalone cotomet file: its declarations, then its entries.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    /// The `::name` declarations at the top, in order.
    pub header: Vec<Declaration>,
    /// The entries: a [`Value::Map`] of `key: value`.
    pub body: Value,
}

/// Parse a standalone cotomet file (`.ctmt`): declarations, then a map of
/// `key: value` entries.
///
/// The body is [`parse_value`] with the one rule that makes a file a file:
/// every entry has a key. A source that is a lone scalar (`123`), or that has
/// a bare value among its entries (`a: 1`, then `stray`), is an error at that
/// entry, where `parse_value` would read the first as a scalar and keep the
/// second as a positional entry. A declaration below the first entry is an
/// error too: they go at the top. An empty file, or one with only comments, has
/// no declarations and the empty map.
pub fn parse_file(src: &str) -> Result<File> {
    let mut cur = Cursor::new(src);
    let mut header = Vec::new();
    loop {
        skip_ws_newlines_and_comments(&mut cur);
        if !cur.starts_with("::") {
            break;
        }
        header.push(parse_declaration(&mut cur).map_err(|e| e.located_in(src))?);
    }
    let body = parse_map_body_impl(&mut cur, &mut NoHook, true).map_err(|e| e.located_in(src))?;
    skip_ws_newlines_and_comments(&mut cur);
    if !cur.is_eof() {
        return Err(err(cur.pos(), "unexpected trailing content").located_in(src));
    }
    Ok(File { header, body })
}

/// One `::name(args){data}` declaration, ending its line. The `::` is next.
fn parse_declaration(cur: &mut Cursor) -> Result<Declaration> {
    let offset = cur.pos();
    cur.eat_str("::");
    let Some(name) = eat_name(cur) else {
        return Err(err(offset, "expected a name after '::'"));
    };
    let (mut args, mut data) = (None, None);
    loop {
        skip_inline_ws(cur);
        match cur.peek() {
            Some('(') if args.is_none() => {
                cur.bump();
                args = Some(parse_call_args(cur, &mut NoHook, false)?);
            }
            Some('{') if data.is_none() => {
                cur.bump();
                let map = parse_map_body_impl(cur, &mut NoHook, false)?;
                skip_ws_newlines_and_comments(cur);
                if !cur.eat_str("}") {
                    return Err(err(cur.pos(), "expected '}'"));
                }
                data = Some(map);
            }
            _ => break,
        }
    }
    skip_inline_ws(cur);
    if cur.starts_with("//") {
        skip_line_comment(cur);
    }
    if !matches!(cur.peek(), None | Some('\n') | Some('\r')) {
        return Err(err(cur.pos(), "unexpected content after a declaration"));
    }
    Ok(Declaration {
        name: name.to_string(),
        args: args.unwrap_or_default(),
        data,
        offset,
    })
}

fn parse_map_body_impl<H: ValueHook>(
    cur: &mut Cursor,
    hook: &mut H,
    keyed_only: bool,
) -> Result<Value> {
    let mut entries: Vec<(String, Value)> = Vec::new();
    let mut seen: Option<HashSet<String>> = None;
    loop {
        skip_ws_newlines_and_comments(cur);
        if matches!(cur.peek(), None | Some(')') | Some('}') | Some(']')) {
            break;
        }
        let entry_start = cur.pos();
        if keyed_only && cur.starts_with("::") {
            return Err(err(
                entry_start,
                "a declaration `::name` goes at the top of the file, before the entries",
            ));
        }
        let (key, value) = parse_one_entry_with(cur, hook)?;
        if keyed_only && key == POSITIONAL_ENTRY_KEY {
            return Err(err(
                entry_start,
                "expected a `key: value` entry, found a bare value",
            ));
        }
        if key != POSITIONAL_ENTRY_KEY {
            let duplicate = if entries.len() < KEY_SCAN_LIMIT {
                entries.iter().any(|(k, _)| *k == key)
            } else {
                let seen =
                    seen.get_or_insert_with(|| entries.iter().map(|(k, _)| k.clone()).collect());
                !seen.insert(key.clone())
            };
            if duplicate {
                return Err(duplicate_key_error(entry_start, &key));
            }
        }
        entries.push((key, value));
        skip_inline_ws(cur);
        if cur.starts_with("//") {
            skip_line_comment(cur);
        }
        match cur.peek() {
            Some(',') => {
                cur.bump();
            }
            Some('\n') | Some('\r') => {}
            None | Some(')' | '}' | ']') => break,
            _ => {
                return Err(err(cur.pos(), "expected ',' or a newline between entries"));
            }
        }
    }
    Ok(Value::Map(entries))
}

pub fn parse_entry_value(cur: &mut Cursor) -> Result<Value> {
    parse_entry_value_with(cur, &mut NoHook)
}

pub fn parse_entry_value_with<H: ValueHook>(cur: &mut Cursor, hook: &mut H) -> Result<Value> {
    parse_entry_value_impl(cur, hook, false)
}

/// [`parse_entry_value_with`], optionally taking a bare scalar as its written
/// text (a `Value::String`) instead of typing it. `string(...)` reads its
/// arguments that way.
fn parse_entry_value_impl<H: ValueHook>(
    cur: &mut Cursor,
    hook: &mut H,
    raw_scalars: bool,
) -> Result<Value> {
    skip_ws_newlines_and_comments(cur);
    if let Some(result) = hook.try_parse_value(cur) {
        return result;
    }
    if let Some(result) = try_parse_call_with(cur, hook) {
        return result;
    }
    match cur.peek() {
        Some('[') => Err(err(
            cur.pos(),
            "'[...]' list literal was removed -- write 'list(...)' instead",
        )),
        Some('"') => Ok(Value::String(parse_quoted(cur)?)),
        Some('{') => {
            cur.bump();
            let v = parse_map_body_with(cur, hook)?;
            skip_ws_newlines_and_comments(cur);
            if !cur.eat_str("}") {
                return Err(err(cur.pos(), "expected '}'"));
            }
            Ok(v)
        }
        _ => {
            let start = cur.pos();
            let raw = eat_scalar_raw(cur).trim();
            if raw.is_empty() {
                return Err(err(cur.pos(), "expected a value"));
            }
            if raw_scalars {
                return Ok(Value::String(raw.to_string()));
            }
            scalar_from_text(raw).map_err(|message| err(start, message))
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
        return Err(err(checkpoint, "expected a value"));
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
