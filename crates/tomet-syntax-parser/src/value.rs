//! Parsing for `Value`: map bodies (`key: value`), `[..]` sequences,
//! `"..."` strings, and bare scalars. Shared by the data-only entry point
//! (`parse_value`) and by `document.rs` for `(args)` groups and
//! data-shaped `{value}` groups.

use crate::error::{Error, Result};
use tomet_ast::{Name, Value};
use tomet_lexer::Cursor;

/// Parse an entire source string as one `Value` (a data-only `.tmt`
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

/// Characters legal inside a **map key**.
///
/// Deliberately wider than an element name (see [`is_name_start`] /
/// [`is_name_char`]): keys are Unicode and may contain `.`, because dotted
/// keys are one flat key here -- `default.config.tmt` has `url.wiki`, and
/// `tomet-semantics`' config reader prefix-matches `macros.`. Element names
/// use `.` as the namespace separator instead, so the two cannot share a
/// lexer.
pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

/// First character of one segment of an element name.
///
/// Element names are ASCII: `[A-Za-z_][A-Za-z0-9_-]*`, joined by `.` into
/// `namespace.name`. Keeping them ASCII means a non-ASCII `#タグ` never
/// lexes as an element and stays prose, which is the right default for the
/// Japanese docs, and it matches `tree-sitter-tomet`'s existing regex.
pub(crate) fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Subsequent characters of one segment of an element name.
pub(crate) fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
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

/// Reads one `.`-separated element name, or `None` if `cur` is not at a
/// name start.
///
/// A trailing `.` is not consumed: `#a.` reads the name `a` and leaves the
/// `.` for whatever follows, rather than erroring, so the fall-back-to-text
/// rule can still claim the line.
pub(crate) fn eat_name(cur: &mut Cursor) -> Option<Name> {
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
    // Only the last segment is the name; everything before it is the
    // namespace path, joined back with `.`.
    let name = segments.pop().expect("at least one segment").to_string();
    let namespace = if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    };
    Some(Name { namespace, name })
}

fn eat_name_segment<'a>(cur: &mut Cursor<'a>) -> Option<&'a str> {
    if !cur.peek().is_some_and(is_name_start) {
        return None;
    }
    Some(cur.eat_while(is_name_char))
}

/// Whether `cur` is at the start of an element name, without consuming it.
pub(crate) fn is_name_start_at(cur: &Cursor) -> bool {
    cur.peek().is_some_and(is_name_start)
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
/// `list(card, ns.mycard)`. Recognized by "a bare identifier immediately
/// followed by `(`, no gap" -- deliberately the same, narrower recognizer
/// `@name`/`:name` use ([`eat_name_segment`]), not the wider map-key
/// charset [`eat_ident`] accepts (which allows `-`/`.`): a call's callee
/// is a single plain word -- `list`, `enum`, a typo -- never namespaced,
/// and the parser does not judge which callees are real (that is
/// `tomet-semantics`' closed table, the same split `:name(...)` connects
/// use).
///
/// Returns `None` and leaves `cur` untouched when the shape doesn't
/// match, so a bare-scalar fallback can still run unaffected (`card-name`
/// with no trailing `(` is not a call). A call's own arguments are
/// positional only -- each parsed the same way a value after `key:` is
/// ([`parse_entry_value`]), recursively, so nested calls
/// (`list(a, list(b))`) and namespaced argument text (`ns.mycard`) both
/// fall out for free without this function needing to know about either.
fn try_parse_call(cur: &mut Cursor) -> Option<Result<Value>> {
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
        match parse_entry_value(cur) {
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
/// `tomet-semantics::positional` folds entries under this key into
/// whichever element-specific positional slot they belong to (e.g.
/// `@link`'s `target`, or a `@settings`-defined custom element's own
/// `positional:[...]` list).
pub(crate) const POSITIONAL_ENTRY_KEY: &str = "";

/// One entry inside a map body: an explicit `key: value` (Group A's
/// `identifier:` rule, `://` as the sole exception -- see
/// `is_scheme_uri_colon` -- and a leading `/` as the sole
/// always-bare-scalar exception, see `starts_absolute_path`), or, if
/// neither of those shapes matches, a bare positional value with no key at
/// all, tagged with [`POSITIONAL_ENTRY_KEY`].
///
/// The positional case delegates to [`parse_entry_value`] rather than
/// reading a raw scalar directly, so `(a, "b")` and `("a", b)` parse the
/// quoted entry the same way a `key: "b"` value would (`Value::String`,
/// unescaped) regardless of which position it sits in. It used to call
/// `eat_scalar_raw` unconditionally, which does not know about quoting at
/// all -- a positional value was only ever unescaped when it happened to
/// be the sole value in the group, via [`parse_value_at`]'s old fast path
/// for a leading `"`; anywhere else `"star"` came back as the four-character
/// *text* `"star"`, quote marks included, rather than the three-letter
/// string `star`.
pub(crate) fn parse_one_entry(cur: &mut Cursor) -> Result<(String, Value)> {
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
    let value = parse_entry_value(cur)?;
    Ok((POSITIONAL_ENTRY_KEY.to_string(), value))
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
    // A real, `@`-sigiled element sitting in a value slot -- see
    // `Value::Element`'s doc comment for what distinguishes this from
    // `try_parse_call` below (an inert, uninterpreted literal). Checked
    // first: `try_parse_call`'s `eat_name_segment` never matches `@`
    // anyway (it isn't a name-start character), so this can't shadow it,
    // but the element check is the one whose intent this states.
    // `allow_colon_connect: false`, same as a bare element inside a
    // `{...}` group's `Entry::Element` -- a value position never takes
    // `:name(...)`. `Placement` is left at `parse_element`'s own default
    // (`Inline`): an embedded value is never promoted to a block just
    // because it happens to start a source line, the way a bare `{...}`
    // entry or a document-level element can be.
    if cur.peek() == Some('@') && crate::element::is_element_start(cur, false) {
        let start = cur.pos();
        let el = crate::element::parse_element(cur, false)?;
        // MVP scope: `(args)` only, no `[content]`/`{value}`/children. The
        // motivating case (`@doc.icon("triangle")`) never needs them, and
        // printing one back out (`tomet-format-style`) would need its own
        // inline-content renderer with sigil-escaping -- real work that
        // belongs with a caller that actually wants it, not invented
        // speculatively here. Rejected rather than silently accepted and
        // printed wrong: a round-trip that doesn't reparse to the same
        // tree is worse than an error at the source that caused it.
        if el.content.is_some() || el.value.is_some() || el.children.is_some() {
            return Err(err(
                cur,
                start,
                "an element embedded in a value may only take (args) -- \
                 [content]/{value} on a value-embedded element isn't supported yet",
            ));
        }
        return Ok(Value::Element(Box::new(el)));
    }
    if let Some(result) = try_parse_call(cur) {
        return result;
    }
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
/// `{...}` nested map) are handled identically for [`parse_seq_item`],
/// which is the only caller left: each stops right after that one value,
/// never looking past it for a `,` (`[a, b]` must stay two items, not
/// collapse into one two-entry map). [`parse_value_at`] used to share this
/// same fast path and, with it, the same "stop right after" behavior --
/// which was wrong there: a leading `"star"` in `("star", pkg:"x")` isn't
/// the whole group, so it went straight to expecting `)` and choked on the
/// `,`. It calls [`parse_map_body_or_scalar`] directly now, which reads a
/// leading quoted/seq/map value as entry zero and keeps going.
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
    parse_map_body_or_scalar(cur)
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
