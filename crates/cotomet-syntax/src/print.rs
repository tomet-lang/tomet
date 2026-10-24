//! Renders a `Value` back into cotomet data text:
//! a top-level map is written as bare `key: value` lines (no enclosing braces);
//! nested maps use `{ ... }`, and a sequence prints as `list(...)`.

use tomet_ast::Value;

use crate::parser::{is_ident_char, scalar_from_text};

pub fn print_value(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out, true);
    out
}

fn write_value(value: &Value, out: &mut String, top_level: bool) {
    match value {
        Value::Map(entries) => write_map(entries, out, top_level),
        Value::Seq(items) => {
            out.push_str("list(");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_value(item, out, false);
            }
            out.push(')');
        }
        Value::String(s) => write_scalar_string(s, out),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => write_float(*f, out),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Null => out.push_str("null"),
        Value::Call(name, args) => {
            out.push_str(name);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_value(arg, out, false);
            }
            out.push(')');
        }
        Value::Element(el) => {
            out.push('@');
            out.push_str(&el.sigil.name().map(|n| n.to_string()).unwrap_or_default());
            if let Some(args) = &el.args {
                out.push('(');
                write_args(args, out);
                out.push(')');
            }
        }
        // Best-effort only: `print_value` has no document-level block
        // renderer to call (it's a generic cotomet-data printer, used by
        // `cotomet`'s own tests/convenience function, not by
        // `tomet-format-printer`, which has its own real block renderer
        // and never calls through here). Plain text keeps this
        // exhaustive without pretending to round-trip nested structure.
        Value::Blocks(blocks) => {
            out.push('[');
            for (i, block) in blocks.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                write_block_plain_text(block, out);
            }
            out.push(']');
        }
    }
}

fn write_block_plain_text(block: &tomet_ast::Block, out: &mut String) {
    use tomet_ast::{Block, Inline};
    let Block::Paragraph(p) = block else {
        return;
    };
    for inline in &p.content {
        match inline {
            Inline::Text(t) => out.push_str(&t.value),
            Inline::Raw(r) => out.push_str(&r.value),
            Inline::SoftBreak(_) => out.push(' '),
            Inline::LineBreak(_) => out.push('\n'),
            Inline::Element(_) => {}
        }
    }
}

fn write_args(value: &Value, out: &mut String) {
    let Value::Map(entries) = value else {
        write_value(value, out, false);
        return;
    };
    for (i, (key, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if !key.is_empty() {
            write_key(key, out);
            out.push_str(": ");
        }
        write_value(v, out, false);
    }
}

fn write_map(entries: &[(String, Value)], out: &mut String, top_level: bool) {
    if top_level {
        for (i, (key, value)) in entries.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            write_key(key, out);
            out.push_str(": ");
            write_value(value, out, false);
        }
    } else {
        out.push('{');
        for (i, (key, value)) in entries.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            write_key(key, out);
            out.push_str(": ");
            write_value(value, out, false);
        }
        out.push('}');
    }
}

fn is_iso8601(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() >= 10 && bytes[4] == b'-' && bytes[7] == b'-' {
        let is_date = s[0..4].chars().all(|c| c.is_ascii_digit())
            && s[5..7].chars().all(|c| c.is_ascii_digit())
            && s[8..10].chars().all(|c| c.is_ascii_digit());
        if !is_date {
            return false;
        }
        if bytes.len() == 10 {
            return true;
        }
        if (bytes[10] == b'T' || bytes[10] == b' ') && bytes.len() >= 19 {
            return bytes[13] == b':' && bytes[16] == b':';
        }
    }
    false
}

/// Writes a float so it reads back as the same float, never as an int:
/// `1.0` stays `1.0` and `1e21` stays `1e21`. A non-finite float has no bare
/// spelling, so it is written with the `float(...)` constructor
/// (`float(nan)`, `float(inf)`, `float(-inf)`).
pub fn write_float(f: f64, out: &mut String) {
    if f.is_nan() {
        out.push_str("float(nan)");
    } else if f.is_infinite() {
        out.push_str(if f > 0.0 { "float(inf)" } else { "float(-inf)" });
    } else {
        out.push_str(&format!("{f:?}"));
    }
}

/// [`write_float`] as an owned string, for callers that build with `format!`.
pub fn float_text(f: f64) -> String {
    let mut out = String::new();
    write_float(f, &mut out);
    out
}

pub fn write_scalar_string(s: &str, out: &mut String) {
    if is_iso8601(s) {
        out.push_str(s);
        return;
    }

    if (s.starts_with("${") && s.ends_with('}'))
        || (s.starts_with('$') && s.contains('(') && s.ends_with(')'))
    {
        out.push_str(s);
        return;
    }

    let first_char = s.chars().next();
    let starts_with_special = matches!(
        first_char,
        Some('&' | '*' | '!' | '%' | '@' | '`' | '|' | '>' | '?' | '-' | '~')
    );

    // Quoted unless the bare text reads back as this same string: a keyword,
    // a number, and an out-of-range number (which would not read back at all)
    // all need quotes.
    let needs_quotes = s.is_empty()
        || !matches!(scalar_from_text(s), Ok(Value::String(_)))
        || s.trim() != s
        || starts_with_special
        || s.contains([
            '"', '\'', ':', ',', '(', ')', '[', ']', '{', '}', '\n', '\r', '\t', ' ',
        ]);

    if needs_quotes {
        write_quoted(s, out);
    } else {
        out.push_str(s);
    }
}

/// Writes a map key: bare when it is a non-empty run of [`is_ident_char`],
/// quoted (`"tmtroot/a/b.tmt"`) otherwise, so the output reads back as the
/// same key. Every printer that emits `key: value` goes through here.
///
/// An empty key is [`crate::POSITIONAL_ENTRY_KEY`] and has no keyed spelling; callers
/// print such an entry as a bare value. It is quoted here only so a stray one
/// fails to parse instead of reading back as something else.
pub fn write_key(key: &str, out: &mut String) {
    if !key.is_empty() && key.chars().all(is_ident_char) {
        out.push_str(key);
    } else {
        write_quoted(key, out);
    }
}

/// [`write_key`] as an owned string, for callers that build with `format!`.
pub fn key_text(key: &str) -> String {
    let mut out = String::new();
    write_key(key, &mut out);
    out
}

/// `s` as a `"..."` string whatever it holds, where [`key_text`] quotes only what
/// has to be. For a place that uses `.` as a separator: a bare key may hold one.
pub fn quoted_text(s: &str) -> String {
    let mut out = String::new();
    write_quoted(s, &mut out);
    out
}

/// Writes `s` as a `"..."` string, with the escapes `parse_quoted` reads.
fn write_quoted(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
}
