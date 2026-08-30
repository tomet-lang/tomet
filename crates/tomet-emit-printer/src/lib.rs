//! Serializes a `tomet_ast::Document` back to `.tmt` source text
//! (no span info required, unlike `tomet-formatter`, which
//! re-formats existing `.tmt` text losslessly using spans -- this crate
//! is for documents that never had `.tmt` source to begin with, e.g.
//! ones built from Markdown or edited purely at the AST level). Uses
//! `tomet_config::PrinterConfig` (see that crate for config
//! loading/discovery) to drive formatting choices: meta format
//! (yaml/json/toml), link-key spacing, callout/list style, via
//! `tomet-style`'s single-node renderers. Also owns `@meta` id
//! auto-generation (`ensure_document_id_with_config`, which mutates the
//! AST directly), built on the pure generate/validate/convert helpers
//! in `tomet-field-utils`.

use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, InterpExpr, InterpExprKind, Literal, Sigil,
    Value,
};
use tomet_config::PrinterConfig;
use tomet_field_utils::{generate_id_for_field, is_valid_id_format};
use tomet_semantics::{ElementKind, classify, heading_level, list_items, list_ordered};
use tomet_style::{render_args_with_config, render_value, render_value_inner_with_config};

pub fn ensure_document_id_with_config(doc: &mut Document, config: &PrinterConfig) {
    let Some(id_cfg) = config.meta_fields.get("id") else {
        return;
    };
    if id_cfg.field_type.is_none() {
        return;
    }

    let force = id_cfg.force.unwrap_or(true);
    let overwrite = id_cfg.overwrite.unwrap_or(false);

    let mut meta_found = false;
    for block in &mut doc.blocks {
        if let Block::Element(el) = block {
            if tomet_semantics::classify(el) == tomet_semantics::ElementKind::Meta
                || matches!(&el.sigil, Sigil::At(Some(name)) if name == "meta")
            {
                meta_found = true;
                if let Some(ElementValue::Data(Value::Map(entries))) = &mut el.value {
                    let mut existing_idx = None;
                    for (idx, (k, v)) in entries.iter().enumerate() {
                        if k == "id" {
                            existing_idx = Some((idx, v.clone()));
                            break;
                        }
                    }

                    if let Some((idx, val)) = existing_idx {
                        if overwrite {
                            let existing_str = match &val {
                                Value::String(s) => s.as_str(),
                                _ => "",
                            };
                            if !is_valid_id_format(existing_str, id_cfg) {
                                let new_id = generate_id_for_field(id_cfg);
                                entries[idx].1 = Value::String(new_id);
                            }
                        }
                    } else if force || overwrite {
                        let new_id = generate_id_for_field(id_cfg);
                        entries.insert(0, ("id".to_string(), Value::String(new_id)));
                    }
                }
                break;
            }
        }
    }

    if !meta_found && (force || overwrite) {
        let new_id = generate_id_for_field(id_cfg);
        let mut meta_el = Element::new(Sigil::At(Some("meta".to_string())));
        meta_el.value = Some(ElementValue::Data(Value::Map(vec![(
            "id".to_string(),
            Value::String(new_id),
        )])));
        doc.blocks.insert(0, Block::Element(meta_el));
    }
}

/// Serialize a [`Document`] AST into Tomet (`.tmt`) source string using document-level config or defaults.
pub fn document_to_tm(doc: &Document) -> String {
    let config = PrinterConfig::from_doc(doc);
    document_to_tm_with_config(doc, &config)
}

pub fn document_to_tm_with_config(doc: &Document, config: &PrinterConfig) -> String {
    let mut out = String::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_block(block, config, &mut out);
    }
    tomet_formatter::format_source(&out)
}

fn render_block(block: &Block, config: &PrinterConfig, out: &mut String) {
    match block {
        Block::Paragraph(p) => {
            let inlines_text = render_inlines(&p.content, config);
            if !inlines_text.trim().is_empty() {
                out.push_str(&inlines_text);
                out.push('\n');
            }
        }
        Block::Element(el) if list_ordered(el).is_some() => render_list(el, config, out),
        Block::Element(el) if classify(el) == ElementKind::Heading => {
            render_heading_element(el, config, out)
        }
        Block::Element(el) => {
            out.push_str(&render_element(el, config));
            out.push('\n');
        }
    }
}

/// Called only from `render_block`'s top-level dispatch, never from the
/// shared, recursively-called `render_element` (which `render_inlines`/
/// `ElementValue::Children` both call for nested/inline elements) -- a
/// nested/inline `@heading(...)` must not round-trip back to `#`-sugar,
/// consistent with `tomet-codegen-html`/`tomet-codegen-markdown`'s
/// equivalent gating for the same resolved decision.
fn render_heading_element(el: &Element, config: &PrinterConfig, out: &mut String) {
    let level = heading_level(el).unwrap_or(1) as usize;
    let content = el.content.as_deref().unwrap_or(&[]);
    out.push_str(&"#".repeat(level));
    if config.heading_space_inside_brackets {
        out.push_str("[ ");
        out.push_str(&render_inlines(content, config));
        out.push_str(" ]");
    } else {
        out.push('[');
        out.push_str(&render_inlines(content, config));
        out.push(']');
    }
    if let Some(ElementValue::Data(v)) = &el.value {
        out.push(' ');
        out.push_str(&render_value(v));
    }
    out.push('\n');
}

fn render_list(el: &Element, config: &PrinterConfig, out: &mut String) {
    render_list_with_indent(el, 0, config, out);
}

fn render_list_with_indent(el: &Element, indent: usize, config: &PrinterConfig, out: &mut String) {
    let ordered = list_ordered(el).unwrap_or(false);
    let indent_str = "  ".repeat(indent);
    for item in list_items(el) {
        let prefix = if ordered {
            "-. ".to_string()
        } else {
            "- ".to_string()
        };
        let mut head_prefix = String::new();
        head_prefix.push_str(&indent_str);
        head_prefix.push_str(&prefix);
        if let Some(marker) = &item.args {
            head_prefix.push('(');
            head_prefix.push_str(&render_args_with_config(marker, config));
            head_prefix.push_str(") ");
        }

        let content_str = render_inlines(item.content.as_deref().unwrap_or(&[]), config);
        let lines: Vec<&str> = content_str.lines().collect();
        let item_attrs = match &item.value {
            Some(ElementValue::Data(v)) => Some(v),
            _ => None,
        };

        if lines.len() > 1 && config.list_multiline_style_content.is_some() {
            let style = config.list_multiline_style_content.as_deref().unwrap();
            let prefix_width = head_prefix.chars().count();
            let pad = " ".repeat(prefix_width + 2);

            out.push_str(&head_prefix);
            if style == "expanded" {
                out.push_str("[\n");
                for line in &lines {
                    out.push_str(&pad);
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push(']');
            } else if style == "block" {
                if lines.len() == 1 {
                    out.push_str("[ ");
                    out.push_str(lines[0]);
                    out.push_str(" ]");
                } else {
                    out.push_str("[ ");
                    for (idx, line) in lines.iter().enumerate() {
                        if idx == 0 {
                            out.push_str(line);
                            out.push('\n');
                        } else {
                            out.push_str(&pad);
                            out.push_str(line);
                            out.push('\n');
                        }
                    }
                    out.push_str(&pad);
                    out.push(']');
                }
            } else if style == "box" {
                out.push_str("[ ");
                for (idx, line) in lines.iter().enumerate() {
                    if idx == 0 {
                        out.push_str(line);
                        out.push('\n');
                    } else if idx == lines.len() - 1 {
                        out.push_str(&pad);
                        out.push_str(line);
                        out.push_str(" ]");
                    } else {
                        out.push_str(&pad);
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            } else {
                out.push_str(&content_str);
            }
            if let Some(attrs) = item_attrs {
                out.push(' ');
                out.push_str(&render_value(attrs));
            }
            out.push('\n');
        } else {
            out.push_str(&head_prefix);
            out.push_str(&content_str);
            if let Some(attrs) = item_attrs {
                out.push(' ');
                out.push_str(&render_value(attrs));
            }
            out.push('\n');
        }
        if let Some(children) = &item.children {
            for child in children {
                if let Block::Element(sub) = child {
                    if list_ordered(sub).is_some() {
                        render_list_with_indent(sub, indent + 1, config, out);
                    }
                }
            }
        }
    }
}

fn render_inlines(inlines: &[Inline], config: &PrinterConfig) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Element(el) => s.push_str(&render_element(el, config)),
        }
    }
    s
}

/// Render an [`Element`] AST node into Tomet syntax: `<sigil>(args)[content]{value}`
pub fn render_element(el: &Element, config: &PrinterConfig) -> String {
    if let Sigil::Type(name) = &el.sigil {
        if name == "hr" && el.args.is_none() && el.value.is_none() {
            if let Some(content) = &el.content {
                return format!("---[{}]---", render_inlines(content, config));
            } else {
                return "---".to_string();
            }
        }
    }

    if let Sigil::At(Some(name)) = &el.sigil {
        if name == "meta" {
            return tomet_style::render_meta_element(el, config);
        }
    }

    if let Sigil::At(None) = &el.sigil {
        if el.content.is_some() && el.args.is_some() && el.value.is_none() {
            let mut out = String::from("@");
            if let Some(content) = &el.content {
                out.push('[');
                out.push_str(&render_inlines(content, config));
                out.push(']');
            }
            if let Some(args) = &el.args {
                out.push('(');
                out.push_str(&render_args_with_config(args, config));
                out.push(')');
            }
            return out;
        }
    }

    if matches!(&el.sigil, Sigil::Type(name) if name == "codeblock") {
        let mut out = String::from("<codeblock>");
        if let Some(args) = &el.args {
            out.push('(');
            out.push_str(&render_args_with_config(args, config));
            out.push(')');
        }
        if let Some(content) = &el.content {
            let text = render_inlines(content, config);
            out.push_str("[\n");
            for line in text.lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
            out.push(']');
        }
        if let Some(value) = &el.value {
            out.push('{');
            match value {
                ElementValue::Data(v) => out.push_str(&render_value_inner_with_config(v, config)),
                ElementValue::Children(children) => {
                    for (i, child) in children.iter().enumerate() {
                        if i > 0 {
                            out.push(' ');
                        }
                        out.push_str(&render_element(child, config));
                    }
                }
                ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
            }
            out.push('}');
        }
        return out;
    }

    if matches!(&el.sigil, Sigil::Type(name) if name == "callout") {
        if let Some(style) = config.callout_content_style.as_deref() {
            if style == "expanded" {
                let mut out = String::from("<callout>");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
                    out.push_str("[\n");
                    for line in inlines_text.lines() {
                        out.push_str("  ");
                        out.push_str(line);
                        out.push('\n');
                    }
                    out.push(']');
                }
                if let Some(value) = &el.value {
                    out.push('{');
                    match value {
                        ElementValue::Data(v) => {
                            out.push_str(&render_value_inner_with_config(v, config))
                        }
                        ElementValue::Children(children) => {
                            for (i, child) in children.iter().enumerate() {
                                if i > 0 {
                                    out.push(' ');
                                }
                                out.push_str(&render_element(child, config));
                            }
                        }
                        ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
                    }
                    out.push('}');
                }
                return out;
            } else if style == "block" {
                let mut out = String::from("<callout>");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
                    let lines: Vec<&str> = inlines_text.lines().collect();
                    out.push('\n');
                    if lines.is_empty() {
                        out.push_str("[ ]");
                    } else if lines.len() == 1 {
                        out.push_str("[ ");
                        out.push_str(lines[0]);
                        out.push_str(" ]");
                    } else {
                        for (idx, line) in lines.iter().enumerate() {
                            if idx == 0 {
                                out.push_str("[ ");
                                out.push_str(line);
                                out.push('\n');
                            } else {
                                out.push_str("  ");
                                out.push_str(line);
                                out.push('\n');
                            }
                        }
                        out.push(']');
                    }
                }
                if let Some(value) = &el.value {
                    out.push('{');
                    match value {
                        ElementValue::Data(v) => {
                            out.push_str(&render_value_inner_with_config(v, config))
                        }
                        ElementValue::Children(children) => {
                            for (i, child) in children.iter().enumerate() {
                                if i > 0 {
                                    out.push(' ');
                                }
                                out.push_str(&render_element(child, config));
                            }
                        }
                        ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
                    }
                    out.push('}');
                }
                return out;
            } else if style == "box" {
                let mut out = String::from("<callout>");
                if let Some(args) = &el.args {
                    out.push('(');
                    out.push_str(&render_args_with_config(args, config));
                    out.push(')');
                }
                if let Some(content) = &el.content {
                    let inlines_text = render_inlines(content, config);
                    let lines: Vec<&str> = inlines_text.lines().collect();
                    out.push('\n');
                    if lines.is_empty() {
                        out.push_str("[ ]");
                    } else if lines.len() == 1 {
                        out.push_str("[ ");
                        out.push_str(lines[0]);
                        out.push_str(" ]");
                    } else {
                        for (idx, line) in lines.iter().enumerate() {
                            if idx == 0 {
                                out.push_str("[ ");
                                out.push_str(line);
                                out.push('\n');
                            } else if idx == lines.len() - 1 {
                                out.push_str("  ");
                                out.push_str(line);
                                out.push_str(" ]");
                            } else {
                                out.push_str("  ");
                                out.push_str(line);
                                out.push('\n');
                            }
                        }
                    }
                }
                if let Some(value) = &el.value {
                    out.push('{');
                    match value {
                        ElementValue::Data(v) => {
                            out.push_str(&render_value_inner_with_config(v, config))
                        }
                        ElementValue::Children(children) => {
                            for (i, child) in children.iter().enumerate() {
                                if i > 0 {
                                    out.push(' ');
                                }
                                out.push_str(&render_element(child, config));
                            }
                        }
                        ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
                    }
                    out.push('}');
                }
                return out;
            }
        }
    }

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
        Sigil::Dollar => {
            out.push('$');
        }
    }

    if let Some(args) = &el.args {
        out.push('(');
        out.push_str(&render_args_with_config(args, config));
        out.push(')');
    }

    if let Some(content) = &el.content {
        out.push('[');
        out.push_str(&render_inlines(content, config));
        out.push(']');
    }

    if let Some(value) = &el.value {
        out.push('{');
        match value {
            ElementValue::Data(v) => {
                if let Some(fmt) = get_format_from_args(el.args.as_ref()) {
                    let json_val = value_to_json(v);
                    let serialized = match fmt {
                        "json" => serde_json::to_string_pretty(&json_val).ok(),
                        "yaml" => serde_yaml::to_string(&json_val).ok(),
                        "toml" => toml::to_string_pretty(&json_val).ok(),
                        _ => None,
                    };
                    if let Some(s) = serialized {
                        let trimmed = s.trim();
                        out.push('\n');
                        for line in trimmed.lines() {
                            out.push_str("  ");
                            out.push_str(line);
                            out.push('\n');
                        }
                    } else {
                        out.push_str(&render_value_inner_with_config(v, config));
                    }
                } else {
                    out.push_str(&render_value_inner_with_config(v, config));
                }
            }
            ElementValue::Children(children) => {
                if children.is_empty() {
                    // empty
                } else if children.len() == 1 && children[0].sigil != Sigil::Bare {
                    out.push(' ');
                    out.push_str(&render_element(&children[0], config));
                    out.push(' ');
                } else {
                    out.push('\n');
                    for child in children {
                        out.push_str("  ");
                        out.push_str(&render_element(child, config));
                        out.push('\n');
                    }
                }
            }
            ElementValue::Interp(expr) => out.push_str(&render_interp_expr(expr)),
        }
        out.push('}');
    }

    out
}

fn get_format_from_args(args: Option<&Value>) -> Option<&str> {
    if let Some(Value::Map(entries)) = args {
        for (k, v) in entries {
            if k == "format" {
                if let Value::String(fmt) = v {
                    return Some(fmt.as_str());
                }
            }
        }
    }
    None
}

fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Seq(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(entries) => {
            let mut map = serde_json::Map::new();
            for (k, v) in entries {
                map.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
    }
}

/// Re-renders an `InterpExpr` back to source text for round-tripping
/// (the interior of `${...}`, no surrounding braces -- the caller already
/// adds those as part of `Sigil::Dollar`'s generic `{value}` rendering).
/// `String` is always quoted, unlike `render_value_inner`'s conditional
/// quoting: unlike a `Value::String`, an `InterpExprKind::Literal(Literal::String(_))`
/// only ever comes from an explicitly `"..."`-quoted source token
/// (`document.rs::parse_interp_primary`), never a bare identifier, so it
/// always needs the quotes back.
pub fn render_interp_expr(expr: &InterpExpr) -> String {
    match &expr.kind {
        InterpExprKind::Identifier(name) => name.clone(),
        InterpExprKind::Literal(Literal::Int(i)) => i.to_string(),
        InterpExprKind::Literal(Literal::Float(f)) => f.to_string(),
        InterpExprKind::Literal(Literal::String(s)) => format!("\"{s}\""),
        InterpExprKind::Call { callee, args } => {
            let rendered: Vec<_> = args.iter().map(render_interp_expr).collect();
            format!("{}({})", render_interp_expr(callee), rendered.join(", "))
        }
        InterpExprKind::Member { object, member } => {
            format!("{}.{member}", render_interp_expr(object))
        }
        InterpExprKind::NamedArg { name, value } => {
            format!("{name}: {}", render_interp_expr(value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_config::{FieldConfig, load_config_from_str};
    use tomet_style::render_value_inner;

    #[test]
    fn test_printer_config_space_inside_brackets() {
        let doc = tomet_parser::parse_document("#[Title]\n").unwrap();
        let cfg = PrinterConfig {
            heading_space_inside_brackets: true,
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("#[ Title ]"));
    }

    #[test]
    fn nested_inline_heading_does_not_reserialize_as_hash_sugar() {
        // `@heading(2)[...]` nested inside another element's content must
        // round-trip as a plain `@heading(...)` element, never as `##[...]`
        // -- only `render_block`'s top-level dispatch special-cases
        // headings; the shared, recursively-called `render_element` has no
        // heading arm at all (mirrors `tomet-codegen-html`/
        // `tomet-codegen-markdown`'s equivalent nested-heading tests).
        let doc = tomet_parser::parse_document("<memo>[@heading(2)[Nested]]\n").unwrap();
        let printed = document_to_tm(&doc);
        assert!(!printed.contains("##["), "got: {printed:?}");
        assert!(printed.contains("@heading(2)[Nested]"), "got: {printed:?}");
    }

    #[test]
    fn test_markdown_meta_export_with_meta_format() {
        let md = "---\ntitle: Hello\n---\n\n# World\n";
        let doc = tomet_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("@meta(format:yaml){"));
    }

    #[test]
    fn test_iso8601_timestamp_rendering_in_meta() {
        let md = "---\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = tomet_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("modified: 2026-06-17T05:52:44"));
        assert!(!printed.contains("modified: \"2026-06-17T05:52:44\""));
    }

    #[test]
    fn test_wikilink_rendering() {
        let md = "Check [[name]] and [[name|display]] here.\n";
        let doc = tomet_markdown::from_markdown(md);
        let printed = document_to_tm(&doc);
        assert!(printed.contains("@link(target: \"ref:name\")"));
        assert!(printed.contains("@link(target: \"ref:name\")[display]"));
    }

    #[test]
    fn test_rfc3339_and_aliases_always_newline_meta_field_formatting() {
        let md = "---\naliases:\n  - rust\n  - tomet\ncreated: 2026-06-17T05:52:44\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = tomet_markdown::from_markdown(md);

        let mut meta_fields = std::collections::BTreeMap::new();
        meta_fields.insert(
            "aliases".to_string(),
            FieldConfig {
                field_type: Some("list".to_string()),
                always_newline: true,
                ..Default::default()
            },
        );
        meta_fields.insert(
            "created".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                ..Default::default()
            },
        );
        meta_fields.insert(
            "modified".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                ..Default::default()
            },
        );

        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            meta_fields,
            ..Default::default()
        };

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("created: 2026-06-17T05:52:44Z"));
        assert!(printed.contains("modified: 2026-06-17T05:52:44Z"));
        assert!(printed.contains("aliases:\n    - rust\n    - tomet"));

        let md_empty = "---\naliases: []\nflags: []\n---\n\n# Document\n";
        let doc_empty = tomet_markdown::from_markdown(md_empty);
        let printed_empty = document_to_tm_with_config(&doc_empty, &cfg);
        assert!(printed_empty.contains("aliases: []"));
        assert!(printed_empty.contains("flags: []"));
    }

    #[test]
    fn test_rfc3339_with_offset_formatting() {
        let md = "---\ncreated: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = tomet_markdown::from_markdown(md);

        let mut meta_fields = std::collections::BTreeMap::new();
        meta_fields.insert(
            "created".to_string(),
            FieldConfig {
                field_type: Some("datetime".to_string()),
                format: Some("rfc3339".to_string()),
                offset: Some("+09:00".to_string()),
                ..Default::default()
            },
        );

        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            meta_fields,
            ..Default::default()
        };

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("created: 2026-06-17T05:52:44+09:00"));
    }

    #[test]
    fn test_null_value_rendering() {
        let rendered = render_value_inner(&Value::Null);
        assert_eq!(rendered, "");
        let md = "---\ntitle:\n---\n\n# Document\n";
        let doc = tomet_markdown::from_markdown(md);
        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("title:\n"));
        assert!(!printed.contains("title: null"));
        assert!(!printed.contains("title: \"\""));
    }

    #[test]
    fn test_wikilink_no_space_setting() {
        let md = "Check [[target]] and [[target|display]].\n";
        let doc = tomet_markdown::from_markdown(md);

        let cfg_default = PrinterConfig::default();
        let printed_default = document_to_tm_with_config(&doc, &cfg_default);
        assert!(printed_default.contains("@link(target: \"ref:target\")"));
        assert!(printed_default.contains("@link(target: \"ref:target\")[display]"));

        let cfg_no_space = PrinterConfig {
            link_no_space: true,
            ..Default::default()
        };
        let printed_no_space = document_to_tm_with_config(&doc, &cfg_no_space);
        assert!(printed_no_space.contains("@link(target:\"ref:target\")"));
        assert!(printed_no_space.contains("@link(target:\"ref:target\")[display]"));
    }

    #[test]
    fn test_nanoid_generation_and_ensure_document_id() {
        let settings_src = r#"@settings(format:json){
  {
    "meta": {
      "id": {
        "type": "nanoid",
        "length": 8,
        "prefix": "doc-"
      }
    }
  }
}
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        let id_cfg = cfg.meta_fields.get("id").expect("id config present");
        assert_eq!(id_cfg.field_type.as_deref(), Some("nanoid"));
        assert_eq!(id_cfg.length, Some(8));
        assert_eq!(id_cfg.prefix.as_deref(), Some("doc-"));

        let gen_id = generate_id_for_field(id_cfg);
        assert!(gen_id.starts_with("doc-"));
        assert_eq!(gen_id.len(), 4 + 8); // "doc-" + 8 chars

        let mut doc = tomet_markdown::from_markdown("# Test Note\nHello world\n");
        ensure_document_id_with_config(&mut doc, &cfg);

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("id: doc-"));
    }

    #[test]
    fn test_nanoid_force_and_overwrite_behavior() {
        let settings_src = r#"@settings(format:json){
  {
    "meta": {
      "id": {
        "type": "nanoid",
        "length": 8,
        "prefix": "doc-",
        "force": true,
        "overwrite": true
      }
    }
  }
}
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        let id_cfg = cfg.meta_fields.get("id").unwrap();
        assert_eq!(id_cfg.force, Some(true));
        assert_eq!(id_cfg.overwrite, Some(true));

        // Case 1: Valid existing ID is kept
        let md1 = "---\nid: doc-12345678\n---\n# Note 1\n";
        let mut doc1 = tomet_markdown::from_markdown(md1);
        ensure_document_id_with_config(&mut doc1, &cfg);
        let printed1 = document_to_tm_with_config(&doc1, &cfg);
        assert!(printed1.contains("id: doc-12345678"));

        // Case 2: Invalid format existing ID is overwritten when overwrite: true
        let md2 = "---\nid: invalid-slug\n---\n# Note 2\n";
        let mut doc2 = tomet_markdown::from_markdown(md2);
        ensure_document_id_with_config(&mut doc2, &cfg);
        let printed2 = document_to_tm_with_config(&doc2, &cfg);
        assert!(!printed2.contains("invalid-slug"));
        assert!(printed2.contains("id: doc-"));

        // Case 3: When force: true and overwrite: false, invalid existing ID is preserved
        let cfg_no_overwrite = PrinterConfig {
            meta_fields: std::collections::BTreeMap::from([(
                "id".to_string(),
                FieldConfig {
                    field_type: Some("nanoid".to_string()),
                    length: Some(8),
                    prefix: Some("doc-".to_string()),
                    force: Some(true),
                    overwrite: Some(false),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let mut doc3 = tomet_markdown::from_markdown(md2);
        ensure_document_id_with_config(&mut doc3, &cfg_no_overwrite);
        let printed3 = document_to_tm_with_config(&doc3, &cfg_no_overwrite);
        assert!(printed3.contains("id: invalid-slug"));
    }

    #[test]
    fn test_obsidian_callout_blockquote_printing() {
        let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n";
        let doc = tomet_markdown::from_markdown(md);
        let printed = document_to_tm_with_config(&doc, &PrinterConfig::default());
        assert!(printed.contains("<callout>(info, title: \"2025/04/29 11:09\")["));
        assert!(printed.contains("コレさすがに草www"));

        let md_plain = "> Plain quote text\n";
        let doc_plain = tomet_markdown::from_markdown(md_plain);
        let printed_plain = document_to_tm_with_config(&doc_plain, &PrinterConfig::default());
        assert!(printed_plain.contains("<blockquote>["));
        assert!(printed_plain.contains("Plain quote text"));
        assert!(!printed_plain.contains("<blockquote>("));
    }

    #[test]
    fn test_callout_content_style_formatting() {
        let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n> お前なら[[2025-04-26|どうするんだ]]？\n";
        let doc = tomet_markdown::from_markdown(md);

        // Test "block" style
        let cfg_block = PrinterConfig {
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        };
        let printed_block = document_to_tm_with_config(&doc, &cfg_block);
        assert!(printed_block.contains("<callout>(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？\n]"));

        // Test "box" style
        let cfg_box = PrinterConfig {
            callout_content_style: Some("box".to_string()),
            ..Default::default()
        };
        let printed_box = document_to_tm_with_config(&doc, &cfg_box);
        assert!(printed_box.contains("<callout>(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？ ]"));

        // Test "expanded" style
        let cfg_expanded = PrinterConfig {
            callout_content_style: Some("expanded".to_string()),
            ..Default::default()
        };
        let printed_expanded = document_to_tm_with_config(&doc, &cfg_expanded);
        assert!(printed_expanded.contains("<callout>(info, title: \"2025/04/29 11:09\")[\n  コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？\n]"));
    }

    #[test]
    fn test_list_multiline_style_formatting() {
        let md = "1. いや、まずこういう話をするときの前提として、\n   Vtuberでくくってるやつがまず、ゴミだ。\n   確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。\n";
        let doc = tomet_markdown::from_markdown(md);

        let cfg_box = PrinterConfig {
            list_multiline_style_content: Some("box".to_string()),
            ..Default::default()
        };
        let printed_box = document_to_tm_with_config(&doc, &cfg_box);
        assert!(printed_box.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。 ]"));

        let cfg_block = PrinterConfig {
            list_multiline_style_content: Some("block".to_string()),
            ..Default::default()
        };
        let printed_block = document_to_tm_with_config(&doc, &cfg_block);
        assert!(printed_block.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。\n     ]"));
    }

    #[test]
    fn test_single_line_block_style_formatting() {
        let md_callout = "> [!info] Single Line\n> 一行テキスト\n";
        let doc_callout = tomet_markdown::from_markdown(md_callout);
        let cfg = PrinterConfig {
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc_callout, &cfg);
        assert!(printed.contains("<callout>(info, title: \"Single Line\")\n[ 一行テキスト ]"));
    }

    #[test]
    fn test_link_no_space_setting() {
        let md = "[Google](https://google.com)\n";
        let doc = tomet_markdown::from_markdown(md);

        let cfg_default = PrinterConfig::default();
        let printed_default = document_to_tm_with_config(&doc, &cfg_default);
        assert!(printed_default.contains("@link(target: \"https://google.com\")[Google]"));

        let cfg_nospace = PrinterConfig {
            link_no_space: true,
            ..Default::default()
        };
        let printed_nospace = document_to_tm_with_config(&doc, &cfg_nospace);
        assert!(printed_nospace.contains("@link(target:\"https://google.com\")[Google]"));
    }

    #[test]
    fn test_codeblock_formatting() {
        let md = "```shell\nirm \"https://christitus.com/win\" | iex\n```\n";
        let doc = tomet_markdown::from_markdown(md);
        let printed = document_to_tm(&doc);
        assert_eq!(
            printed.trim(),
            "<codeblock>(shell)[\n  irm \"https://christitus.com/win\" | iex\n]"
        );
    }

    #[test]
    fn test_embedded_format_serialization_and_reparse() {
        let mut el = Element::new(Sigil::At(Some("config".to_string())));
        el.args = Some(Value::Map(vec![("format".to_string(), Value::String("json".to_string()))]));
        el.value = Some(ElementValue::Data(Value::Map(vec![
            ("meta".to_string(), Value::String("yaml".to_string())),
        ])));
        let doc = Document::new(vec![Block::Element(el)], tomet_ast::Span::dummy());
        let printed = document_to_tm(&doc);
        assert!(printed.contains("\"meta\": \"yaml\""));

        // Verify re-parsing
        let re_parsed = tomet_parser::parse_document(&printed).expect("valid doc");
        assert_eq!(re_parsed.blocks.len(), 1);
    }

    #[test]
    fn test_container_children_indentation() {
        let mut child1 = Element::new(Sigil::Bare);
        child1.args = Some(Value::Int(1));
        child1.content = Some(vec![Inline::Text("note 1".into())]);

        let mut child2 = Element::new(Sigil::Bare);
        child2.args = Some(Value::Int(2));
        child2.content = Some(vec![Inline::Text("note 2".into())]);

        let mut links = Element::new(Sigil::At(Some("links".to_string())));
        links.value = Some(ElementValue::Children(vec![child1, child2]));

        let doc = Document::new(vec![Block::Element(links)], tomet_ast::Span::dummy());
        let printed = document_to_tm(&doc);
        assert!(printed.contains("@links{\n  (1)[note 1]\n  (2)[note 2]\n}"));

        // Verify re-parsing
        let re_parsed = tomet_parser::parse_document(&printed).expect("valid doc");
        assert_eq!(re_parsed.blocks.len(), 1);
    }
}
