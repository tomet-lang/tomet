//! Renders a `Value` back into cotomet data text:
//! a top-level tuple whose entries all have names is written as bare `key: value`
//! lines (no enclosing parentheses); any other tuple is `( ... )`, with `key: value`
//! for a named entry and the value alone for an unnamed one.

use tomet_ast::{Tuple, Value};
use tomet_spelling::{write_key, write_scalar_string};

pub fn print_value(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out, true);
    out
}

fn write_value(value: &Value, out: &mut String, top_level: bool) {
    match value {
        Value::Tuple(tuple) => {
            if tuple.is_empty() {
                if !top_level {
                    out.push_str("()");
                }
            } else if top_level && tuple.has_no_unnamed() {
                write_top_level_lines(tuple, out);
            } else {
                out.push('(');
                write_args(tuple, out);
                out.push(')');
            }
        }
        Value::String(s) => write_scalar_string(s, out),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => write_float(*f, out),
        Value::Number(text) => out.push_str(text),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::None => out.push_str("none"),
        Value::Auto => out.push_str("auto"),
        Value::Call(name, args) => {
            out.push_str(name);
            out.push('(');
            write_args(args, out);
            out.push(')');
        }
        Value::Interp(interp) => {
            out.push_str(&interp.to_string());
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
        Value::TypeSeat(seat) => {
            out.push_str(&seat.to_string());
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
            Inline::Element(_) | Inline::Interp(_) | Inline::Hole(_) | Inline::TypeSeat(_) => {}
        }
    }
}

fn write_args(tuple: &Tuple, out: &mut String) {
    for (i, entry) in tuple.entries().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if let Some(name) = entry.name() {
            write_key(name, out);
            out.push_str(": ");
        }
        write_value(entry.value(), out, false);
    }
}

/// The top level of a document whose entries all have names is written as bare
/// `key: value` lines, with no enclosing `( )`.
fn write_top_level_lines(tuple: &Tuple, out: &mut String) {
    for (i, (key, value)) in tuple.named().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        write_key(key, out);
        out.push_str(": ");
        write_value(value, out, false);
    }
}

pub fn write_float(f: f64, out: &mut String) {
    if f.is_nan() {
        out.push_str("float(nan)");
    } else if f.is_infinite() {
        out.push_str(if f > 0.0 { "float(inf)" } else { "float(-inf)" });
    } else {
        out.push_str(&tomet_scalar::float_text(f));
    }
}

pub fn float_text(f: f64) -> String {
    let mut out = String::new();
    write_float(f, &mut out);
    out
}
