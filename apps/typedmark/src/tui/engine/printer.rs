//! Printer module to serialize `typedmark_ast::Document` back to `.tm` text syntax.

use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, List, Sigil, Value,
};

/// Serialize a [`Document`] AST into TypedMark (`.tm`) source string.
pub fn document_to_tm(doc: &Document) -> String {
    let mut out = String::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_block(block, &mut out);
    }
    typedmark_formatter::format_source(&out)
}

fn render_block(block: &Block, out: &mut String) {
    match block {
        Block::Heading(h) => render_heading(h, out),
        Block::Paragraph(p) => {
            let inlines_text = render_inlines(&p.content);
            if !inlines_text.trim().is_empty() {
                out.push_str(&inlines_text);
                out.push('\n');
            }
        }
        Block::List(list) => render_list(list, out),
        Block::Element(el) => {
            out.push_str(&render_element(el));
            out.push('\n');
        }
    }
}

fn render_heading(h: &Heading, out: &mut String) {
    let level = h.level.clamp(1, 6) as usize;
    out.push_str(&"#".repeat(level));
    out.push('[');
    out.push_str(&render_inlines(&h.content));
    out.push(']');
    if let Some(attrs) = &h.attrs {
        out.push(' ');
        out.push_str(&render_value(attrs));
    }
    out.push('\n');
}

fn render_list(list: &List, out: &mut String) {
    for (i, item) in list.items.iter().enumerate() {
        let prefix = if list.ordered {
            format!("{}. ", i + 1)
        } else {
            "- ".to_string()
        };
        out.push_str(&prefix);
        if let Some(marker) = &item.marker {
            out.push_str(&format!("[{marker}] "));
        }
        out.push_str(&render_inlines(&item.content));
        if let Some(attrs) = &item.attrs {
            out.push(' ');
            out.push_str(&render_value(attrs));
        }
        out.push('\n');
    }
}

fn render_inlines(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Element(el) => s.push_str(&render_element(el)),
        }
    }
    s
}

/// Render an [`Element`] AST node into TypedMark syntax: `<sigil>(input)[area]{value}`
pub fn render_element(el: &Element) -> String {
    let mut out = String::new();

    match &el.sigil {
        Sigil::Type(name) => {
            out.push('<');
            out.push_str(name);
            out.push('>');
        }
        Sigil::At(Some(name)) => {
            out.push('@');
            out.push_str(name);
        }
        Sigil::At(None) => {
            out.push('@');
        }
        Sigil::Bare => {}
    }

    if let Some(input) = &el.input {
        out.push('(');
        out.push_str(&render_value_inner(input));
        out.push(')');
    }

    if let Some(area) = &el.area {
        out.push('[');
        out.push_str(&render_inlines(area));
        out.push(']');
    }

    if let Some(value) = &el.value {
        out.push('{');
        match value {
            ElementValue::Data(v) => out.push_str(&render_value_inner(v)),
            ElementValue::Children(children) => {
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push_str(&render_element(child));
                }
            }
        }
        out.push('}');
    }

    out
}

pub fn render_value(v: &Value) -> String {
    format!("{{{}}}", render_value_inner(v))
}

pub fn render_value_inner(v: &Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => {
            if s.contains(' ') || s.contains(':') || s.contains(',') || s.is_empty() {
                format!("\"{s}\"")
            } else {
                s.clone()
            }
        }
        Value::Seq(items) => {
            let rendered: Vec<_> = items.iter().map(render_value_inner).collect();
            format!("[{}]", rendered.join(", "))
        }
        Value::Map(entries) => {
            let rendered: Vec<_> = entries
                .iter()
                .map(|(k, val)| format!("{k}: {}", render_value_inner(val)))
                .collect();
            rendered.join(", ")
        }
    }
}
