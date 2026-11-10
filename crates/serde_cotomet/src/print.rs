//! Renders a `Value` back into cotomet data text:
//! a top-level map is written as bare `key: value` lines (no enclosing braces);
//! nested maps use `{ ... }`, and a sequence prints as `(a, b)`.

use tomet_ast::Value;
use tomet_spelling::{write_key, write_scalar_string};

pub fn print_value(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out, true);
    out
}

fn write_value(value: &Value, out: &mut String, top_level: bool) {
    match value {
        Value::Map(entries) => write_map(entries, out, top_level),
        Value::Seq(items) => {
            out.push('(');
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
        Value::Number(text) => out.push_str(text),
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
        Value::Interp(interp) => {
            out.push_str(&format!("${{{}}}", interp.expr));
        }
        Value::Hole(hole) => {
            out.push_str("<?");
            if let Some(matcher) = &hole.matcher {
                out.push_str(&matcher.to_string());
            }
            if let Some(args) = &hole.data.args {
                out.push('(');
                write_args(args, out);
                out.push(')');
            }
            out.push('>');
        }
        Value::Element(el) => {
            out.push_str(&format!("@{}", el.name));
            if let Some(args) = &el.data.args {
                out.push('(');
                write_args(args, out);
                out.push(')');
            }
        }
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
            Inline::Element(_) | Inline::Interp(_) | Inline::Hole(_) => {}
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

pub fn write_float(f: f64, out: &mut String) {
    if f.is_nan() {
        out.push_str("float(nan)");
    } else if f.is_infinite() {
        out.push_str(if f > 0.0 { "float(inf)" } else { "float(-inf)" });
    } else {
        out.push_str(&Value::float_text(f));
    }
}

pub fn float_text(f: f64) -> String {
    let mut out = String::new();
    write_float(f, &mut out);
    out
}
