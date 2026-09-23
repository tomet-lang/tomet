//! Renders a `Value` back into TOVE data text:
//! a top-level map is written as bare `key: value` lines (no enclosing braces);
//! nested maps use `{ ... }`, and a sequence prints as `list(...)`.

use tomet_ast::Value;

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
        Value::Float(f) => out.push_str(&f.to_string()),
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
            out.push_str(key);
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
            out.push_str(key);
            out.push_str(": ");
            write_value(value, out, false);
        }
    } else {
        out.push('{');
        for (i, (key, value)) in entries.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(key);
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
        Some('&' | '*' | '!' | '%' | '@' | '`' | '|' | '>' | '?' | '-' | '#' | '~')
    );

    let needs_quotes = s.is_empty()
        || matches!(s, "true" | "false" | "null" | "~")
        || s.parse::<i64>().is_ok()
        || s.parse::<f64>().is_ok()
        || s.trim() != s
        || starts_with_special
        || s.contains([
            '"', '\'', ':', ',', '(', ')', '[', ']', '{', '}', '\n', '\r', '\t', ' ',
        ]);

    if needs_quotes {
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
    } else {
        out.push_str(s);
    }
}
