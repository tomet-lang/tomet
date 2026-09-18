//! Renders a `Value` back into `.tmt` data-mode text: a top-level map is
//! written as bare `key: value` lines (no enclosing braces), matching what
//! `tomet_parser::parse_value` accepts as a whole document; nested maps
//! use `{ }`, and a sequence prints as `list(...)` -- the sole surviving
//! list-value spelling, matching `tomet_style::render_value_inner`.

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
        Value::String(s) => tomet_style::write_scalar_string(s, out),
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
        // Only ever reached by re-printing a `Value` that came from
        // parsing real `.tmt` source (never from serializing a Rust
        // value -- serde's data model has nothing that produces one, see
        // `tomet_ast::Value`'s `Deserialize` impl). `(args)`-only by
        // construction (the parser rejects `[content]`/`{value}` there).
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

/// `(args)`'s bare `key: value, ...` body -- distinct from [`write_map`],
/// which always wraps in `{ }` when not top-level: `(args)` never does,
/// and a positional entry (an empty key -- the parser's sentinel for one,
/// since a real key is never empty) prints as a bare value with no `key:`
/// prefix at all.
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
