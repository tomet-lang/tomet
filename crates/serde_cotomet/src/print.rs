//! Renders a `Value` back into cotomet data text:
//! a top-level tuple whose entries all have names is written as bare `key: value`
//! lines (no enclosing parentheses); any other tuple is `( ... )`, with `key: value`
//! for a named entry and the value alone for an unnamed one.

use tomet_ast::{Tuple, Value};
use tomet_spelling::{write_key, write_scalar};

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
        Value::Scalar(s) => write_scalar(s, out),
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
        match entry.key() {
            Some(tomet_ast::EntryKey::Name(name)) => {
                write_key(name, out);
                out.push_str(": ");
            }
            Some(tomet_ast::EntryKey::TypeSeat(seat)) => {
                out.push_str(&seat.to_string());
                out.push_str(": ");
            }
            None => {}
        }
        write_value(entry.value(), out, false);
    }
}

/// The top level of a document whose entries all have keys is written as bare
/// `key: value` lines, with no enclosing `( )`.
fn write_top_level_lines(tuple: &Tuple, out: &mut String) {
    for (i, entry) in tuple.entries().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match entry.key() {
            Some(tomet_ast::EntryKey::Name(name)) => {
                write_key(name, out);
                out.push_str(": ");
            }
            Some(tomet_ast::EntryKey::TypeSeat(seat)) => {
                out.push_str(&seat.to_string());
                out.push_str(": ");
            }
            None => {}
        }
        write_value(entry.value(), out, false);
    }
}
