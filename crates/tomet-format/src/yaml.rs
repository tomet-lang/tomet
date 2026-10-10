//! YAML quote fixing for bare `@`-led values in `@meta(format:yaml)+++` blocks.

/// Finds every `(...){...}` or `(...)+++` body whose args declare `format:yaml` and
/// wraps any bare, unquoted `@...`-led value inside it in `"..."`.
///
/// Raw-text scanning, not a real YAML parse: cheap, and this only ever needs to recognize
/// three "a value starts here" shapes (`key:`, `- `, and `[`/`,` inside a flow sequence).
pub fn quote_bare_at_yaml_values(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut pos = 0;
    while let Some((body_start, body_end)) = find_next_format_yaml_body(&src[pos..]) {
        let (body_start, body_end) = (pos + body_start, pos + body_end);
        out.push_str(&src[pos..body_start]);
        quote_bare_at_values_in(src, body_start, body_end, &mut out);
        pos = body_end;
    }
    out.push_str(&src[pos..]);
    out
}

/// Byte range (relative to `s`) of the next `format:yaml`-tagged
/// element's `+++` fence body, or `None` if there isn't one.
fn find_next_format_yaml_body(s: &str) -> Option<(usize, usize)> {
    let mut search_from = 0;
    loop {
        let rel_open_paren = s[search_from..].find('(')?;
        let open_paren = search_from + rel_open_paren;
        let Some(close_paren) = find_matching(s, open_paren, '(', ')') else {
            search_from = open_paren + 1;
            continue;
        };
        if args_declare_format_yaml(&s[open_paren + 1..close_paren]) {
            let after = &s[close_paren + 1..];
            let run = after.len() - after.trim_start_matches('+').len();
            if run >= 3 {
                let head_end = close_paren + 1 + run;
                // Skip to just past the newline ending the head line.
                let body_start = {
                    let nl = s[head_end..].find('\n')?;
                    head_end + nl + 1
                };
                let closer = "+".repeat(run);
                let mut scan = body_start;
                while scan < s.len() {
                    let line_end = s[scan..].find('\n').map_or(s.len(), |n| scan + n);
                    if s[scan..line_end].trim_end() == closer {
                        return Some((body_start, scan));
                    }
                    scan = line_end + 1;
                }
                return Some((body_start, s.len()));
            }
        }
        search_from = close_paren + 1;
    }
}

/// Whether an element's `(args)` body (already stripped of the
/// enclosing parens) declares a `format` key of `yaml`, e.g.
/// `format:yaml` or `id:foo, format: yaml`.
fn args_declare_format_yaml(args: &str) -> bool {
    args.split(',').any(|entry| {
        entry
            .split_once(':')
            .is_some_and(|(key, value)| key.trim() == "format" && value.trim() == "yaml")
    })
}

/// Byte offset of the `close` matching the `open` at `open_pos` in
/// `src` (which must be `open`), tracking nested `open`/`close` depth
/// and skipping over `"..."`/`'...'` quoted runs.
pub(crate) fn find_matching(src: &str, open_pos: usize, open: char, close: char) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = open_pos + open.len_utf8();
    let mut depth: u32 = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && c == b'"' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                let closed = bytes[i] == c;
                i += 1;
                if closed {
                    break;
                }
            }
        } else if c == open as u8 {
            depth += 1;
            i += 1;
        } else if c == close as u8 {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Does the actual `@...` -> `"@..."` rewriting within one already-
/// located `format:yaml` body's span `[start, end)`, appending to `out`.
fn quote_bare_at_values_in(src: &str, start: usize, end: usize, out: &mut String) {
    let bytes = src.as_bytes();
    let mut i = start;
    while i < end {
        let c = bytes[i];
        match c {
            b'#' => {
                let line_end = src[i..end].find('\n').map_or(end, |p| i + p);
                out.push_str(&src[i..line_end]);
                i = line_end;
            }
            b'"' | b'\'' => {
                let quote = c;
                let q_start = i;
                i += 1;
                while i < end {
                    if bytes[i] == b'\\' && quote == b'"' && i + 1 < end {
                        i += 2;
                        continue;
                    }
                    let closed = bytes[i] == quote;
                    i += 1;
                    if closed {
                        break;
                    }
                }
                out.push_str(&src[q_start..i]);
            }
            b':' | b',' | b'[' => {
                out.push(c as char);
                i += 1;
                while i < end && matches!(bytes[i], b' ' | b'\t') {
                    out.push(bytes[i] as char);
                    i += 1;
                }
                if i < end && bytes[i] == b'@' {
                    quote_one_value_at(src, &mut i, end, out);
                }
            }
            b'-' if i + 1 < end && matches!(bytes[i + 1], b' ' | b'\t') => {
                out.push('-');
                i += 1;
                while i < end && matches!(bytes[i], b' ' | b'\t') {
                    out.push(bytes[i] as char);
                    i += 1;
                }
                if i < end && bytes[i] == b'@' {
                    quote_one_value_at(src, &mut i, end, out);
                }
            }
            _ => {
                let ch = src[i..].chars().next().expect("i < end <= src.len()");
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
}

/// Helper for [`quote_bare_at_values_in`]: having just seen a `@` at
/// `src[*i]` in a value position, advances `*i` past the end of that
/// scalar value, wrapping the scanned slice in `"..."` as it appends to
/// `out`. Stops at the first newline, unquoted `,`, `]`, `}`, or `#`
/// comment start.
fn quote_one_value_at(src: &str, i: &mut usize, end: usize, out: &mut String) {
    let val_start = *i;
    let bytes = src.as_bytes();
    let mut depth: u32 = 0;
    while *i < end {
        let b = bytes[*i];
        match b {
            b'\n' | b'\r' => break,
            b'#' if depth == 0 => break,
            b'(' | b'[' | b'{' => {
                depth += 1;
                *i += 1;
            }
            b')' | b']' | b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                *i += 1;
            }
            b',' if depth == 0 => break,
            _ => *i += 1,
        }
    }
    let val = src[val_start..*i].trim_end();
    out.push('"');
    out.push_str(val);
    out.push('"');
}
