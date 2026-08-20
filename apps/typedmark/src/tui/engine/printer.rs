//! Printer module to serialize `typedmark_ast::Document` back to `.tm` text syntax.

use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, InterpExpr, InterpExprKind, List,
    Literal, Sigil, Value,
};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldConfig {
    pub field_type: Option<String>,
    pub format: Option<String>,
    pub offset: Option<String>,
    pub always_newline: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrinterConfig {
    pub meta_always_newline: bool,
    pub heading_space_inside_brackets: bool,
    pub meta_format: Option<String>,
    pub meta_fields: std::collections::BTreeMap<String, FieldConfig>,
    pub wikilink_no_space: bool,
}

impl PrinterConfig {
    pub fn from_doc(doc: &Document) -> Self {
        let config = typedmark_semantics::document_config(doc);
        let mut cfg = PrinterConfig::default();

        for (k, v) in &config.entries {
            if k == "format" {
                if let Value::Map(map) = v {
                    Self::apply_format_map(&mut cfg, map);
                }
            } else if let Some(sub_k) = k.strip_prefix("format.") {
                Self::apply_format_key_value(&mut cfg, sub_k, v);
            } else {
                Self::apply_format_key_value(&mut cfg, k, v);
            }
        }

        for block in &doc.blocks {
            if let Block::Element(el) = block {
                if typedmark_semantics::classify(el) == typedmark_semantics::ElementKind::Custom("settings".to_string())
                    || matches!(&el.sigil, Sigil::At(Some(name)) if name == "settings")
                {
                    Self::apply_settings_element(&mut cfg, el);
                }
            }
        }

        cfg
    }

    fn apply_settings_element(cfg: &mut Self, el: &Element) {
        let Some(ElementValue::Data(Value::Map(entries))) = &el.value else {
            return;
        };
        for (k, v) in entries {
            if k == "meta" {
                if let Value::Map(fields) = v {
                    for (field_name, field_val) in fields {
                        if let Value::Map(props) = field_val {
                            let mut field_cfg = cfg.meta_fields.get(field_name).cloned().unwrap_or_default();
                            for (pk, pv) in props {
                                if pk == "type" {
                                    if let Value::String(s) = pv {
                                        field_cfg.field_type = Some(s.clone());
                                    }
                                } else if pk == "format" {
                                    if let Value::String(s) = pv {
                                        field_cfg.format = Some(s.clone());
                                    }
                                } else if pk == "offset" {
                                    if let Value::String(s) = pv {
                                        field_cfg.offset = Some(s.clone());
                                    }
                                } else if pk == "always_newline" {
                                    if let Value::Bool(b) = pv {
                                        field_cfg.always_newline = *b;
                                    }
                                }
                            }
                            cfg.meta_fields.insert(field_name.clone(), field_cfg);
                        }
                    }
                }
            }
            if k == "wikilink" {
                if let Value::Map(props) = v {
                    for (pk, pv) in props {
                        if pk == "no_space" {
                            if let Value::Bool(b) = pv {
                                cfg.wikilink_no_space = *b;
                            } else if let Value::String(s) = pv {
                                cfg.wikilink_no_space = s == "true" || s == "1";
                            }
                        }
                    }
                }
            }
        }
    }

    fn apply_format_map(cfg: &mut Self, map: &[(String, Value)]) {
        for (k, v) in map {
            Self::apply_format_key_value(cfg, k, v);
        }
    }

    fn apply_format_key_value(cfg: &mut Self, key: &str, value: &Value) {
        match key {
            "meta" => {
                if let Value::Map(map) = value {
                    for (mk, mv) in map {
                        if mk == "always_newline" {
                            if let Value::Bool(b) = mv {
                                cfg.meta_always_newline = *b;
                            }
                        } else if mk == "format" {
                            if let Value::String(s) = mv {
                                cfg.meta_format = Some(s.clone());
                            }
                        }
                    }
                }
            }
            "meta.always_newline" => {
                if let Value::Bool(b) = value {
                    cfg.meta_always_newline = *b;
                }
            }
            "meta.format" => {
                if let Value::String(s) = value {
                    cfg.meta_format = Some(s.clone());
                }
            }
            "heading" => {
                if let Value::Map(map) = value {
                    for (hk, hv) in map {
                        if hk == "space_inside_brackets" {
                            if let Value::Bool(b) = hv {
                                cfg.heading_space_inside_brackets = *b;
                            }
                        }
                    }
                }
            }
            "heading.space_inside_brackets" => {
                if let Value::Bool(b) = value {
                    cfg.heading_space_inside_brackets = *b;
                }
            }
            "wikilink" => {
                if let Value::Map(map) = value {
                    for (wk, wv) in map {
                        if wk == "no_space" {
                            if let Value::Bool(b) = wv {
                                cfg.wikilink_no_space = *b;
                            } else if let Value::String(s) = wv {
                                cfg.wikilink_no_space = s == "true" || s == "1";
                            }
                        }
                    }
                }
            }
            "wikilink.no_space" => {
                if let Value::Bool(b) = value {
                    cfg.wikilink_no_space = *b;
                } else if let Value::String(s) = value {
                    cfg.wikilink_no_space = s == "true" || s == "1";
                }
            }
            _ => {}
        }
    }
}

pub fn load_config_from_file(path: &std::path::Path) -> Result<PrinterConfig, String> {
    let src = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    load_config_from_str(&src)
}

pub fn load_config_from_str(src: &str) -> Result<PrinterConfig, String> {
    let doc = typedmark_parser::parse_document(src).map_err(|e| format!("{e:?}"))?;
    Ok(PrinterConfig::from_doc(&doc))
}

/// Serialize a [`Document`] AST into TypedMark (`.tm`) source string using document-level config or defaults.
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
    typedmark_formatter::format_source(&out)
}

fn render_block(block: &Block, config: &PrinterConfig, out: &mut String) {
    match block {
        Block::Heading(h) => render_heading(h, config, out),
        Block::Paragraph(p) => {
            let inlines_text = render_inlines(&p.content, config);
            if !inlines_text.trim().is_empty() {
                out.push_str(&inlines_text);
                out.push('\n');
            }
        }
        Block::List(list) => render_list(list, config, out),
        Block::Element(el) => {
            out.push_str(&render_element(el, config));
            out.push('\n');
        }
    }
}

fn render_heading(h: &Heading, config: &PrinterConfig, out: &mut String) {
    let level = h.level.clamp(1, 6) as usize;
    out.push_str(&"#".repeat(level));
    if config.heading_space_inside_brackets {
        out.push_str("[ ");
        out.push_str(&render_inlines(&h.content, config));
        out.push_str(" ]");
    } else {
        out.push('[');
        out.push_str(&render_inlines(&h.content, config));
        out.push(']');
    }
    if let Some(attrs) = &h.attrs {
        out.push(' ');
        out.push_str(&render_value(attrs));
    }
    out.push('\n');
}

fn render_list(list: &List, config: &PrinterConfig, out: &mut String) {
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
        out.push_str(&render_inlines(&item.content, config));
        if let Some(attrs) = &item.attrs {
            out.push(' ');
            out.push_str(&render_value(attrs));
        }
        out.push('\n');
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

/// Render an [`Element`] AST node into TypedMark syntax: `<sigil>(args)[content]{value}`
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
            let effective_format = el.args.as_ref().and_then(|args| {
                if let Value::Map(entries) = args {
                    entries.iter().find_map(|(k, v)| {
                        if k == "format" {
                            if let Value::String(fmt) = v {
                                return Some(fmt.clone());
                            }
                        }
                        None
                    })
                } else {
                    None
                }
            }).or_else(|| config.meta_format.clone());

            if config.meta_always_newline || effective_format.is_some() {
                if let Some(ElementValue::Data(Value::Map(entries))) = &el.value {
                    let mut out = if let Some(ref fmt) = effective_format {
                        format!("@meta(format:{fmt}){{\n")
                    } else {
                        String::from("@meta{\n")
                    };
                    for (k, v) in entries {
                        let field_cfg = config.meta_fields.get(k);
                        let val_str = render_meta_field_value(v, field_cfg);
                        out.push_str("  ");
                        out.push_str(k);
                        if val_str.starts_with('\n') || val_str.is_empty() {
                            out.push(':');
                        } else {
                            out.push_str(": ");
                        }
                        out.push_str(&val_str);
                        out.push('\n');
                    }
                    out.push('}');
                    return out;
                }
            }
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
            ElementValue::Data(v) => out.push_str(&render_value_inner(v)),
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

    out
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
    }
}

fn is_iso8601(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() >= 10 && bytes[4] == b'-' && bytes[7] == b'-' {
        if bytes.len() == 10 {
            return s[0..4].chars().all(|c| c.is_ascii_digit())
                && s[5..7].chars().all(|c| c.is_ascii_digit())
                && s[8..10].chars().all(|c| c.is_ascii_digit());
        }
        if (bytes[10] == b'T' || bytes[10] == b' ') && bytes.len() >= 19 {
            return s[0..4].chars().all(|c| c.is_ascii_digit())
                && s[5..7].chars().all(|c| c.is_ascii_digit())
                && s[8..10].chars().all(|c| c.is_ascii_digit())
                && bytes[13] == b':'
                && bytes[16] == b':';
        }
    }
    false
}

fn format_rfc3339(s: &str, offset: Option<&str>) -> String {
    let trimmed = s.trim();
    if is_iso8601(trimmed) {
        if !trimmed.ends_with('Z') && !trimmed.contains('+') && trimmed.len() >= 19 && !trimmed[10..].contains('-') {
            let off = offset.unwrap_or("Z");
            format!("{trimmed}{off}")
        } else {
            trimmed.to_string()
        }
    } else {
        trimmed.to_string()
    }
}

fn render_meta_field_value(v: &Value, field_cfg: Option<&FieldConfig>) -> String {
    if let Value::Null = v {
        return String::new();
    }
    if let Some(cfg) = field_cfg {
        if cfg.format.as_deref() == Some("rfc3339") || cfg.field_type.as_deref() == Some("datetime") {
            if let Value::String(s) = v {
                return format_rfc3339(s, cfg.offset.as_deref());
            }
        }
        if cfg.always_newline {
            if let Value::Seq(items) = v {
                let mut s = String::new();
                for item in items {
                    s.push_str("\n    - ");
                    s.push_str(&render_value_inner(item));
                }
                return s;
            }
        }
    }
    match v {
        Value::String(s) if is_iso8601(s) => s.clone(),
        _ => render_value_inner(v),
    }
}

fn render_args_with_config(v: &Value, config: &PrinterConfig) -> String {
    if config.wikilink_no_space {
        if let Value::Map(entries) = v {
            if entries.len() == 1 && entries[0].0 == "wiki" {
                let (k, val) = &entries[0];
                return format!("{k}:{}", render_value_inner(val));
            }
        }
    }
    render_value_inner(v)
}

pub fn render_value(v: &Value) -> String {
    format!("{{{}}}", render_value_inner(v))
}

pub fn render_value_inner(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => {
            if is_iso8601(s) {
                s.clone()
            } else if s.contains(' ') || s.contains(':') || s.contains(',') || s.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_ast::{Heading, Inline, Span, Text};

    #[test]
    fn test_printer_config_space_inside_brackets() {
        let doc = Document {
            blocks: vec![Block::Heading(Heading::new(
                1,
                vec![Inline::Text(Text::new("Title", Span::dummy()))],
                None,
                Span::dummy(),
            ))],
            span: Span::dummy(),
        };
        let cfg = PrinterConfig {
            heading_space_inside_brackets: true,
            ..Default::default()
        };
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("#[ Title ]"));
    }

    #[test]
    fn test_printer_config_meta_format() {
        let config_src = r#"@config(format:json){
          {
            "format": {
              "meta": {
                "format": "yaml"
              }
            }
          }
        }"#;
        let cfg = load_config_from_str(config_src).unwrap();
        assert_eq!(cfg.meta_format.as_deref(), Some("yaml"));
    }

    #[test]
    fn test_markdown_meta_export_with_meta_format() {
        let md = "---\ntitle: Hello\n---\n\n# World\n";
        let doc = typedmark_markdown::from_markdown(md);
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
        let doc = typedmark_markdown::from_markdown(md);
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
        let doc = typedmark_markdown::from_markdown(md);
        let printed = document_to_tm(&doc);
        assert!(printed.contains("@(wiki: name)") || printed.contains("@(wiki:name)"));
        assert!(printed.contains("@[display](wiki: name)") || printed.contains("@[display](wiki:name)"));
    }

    #[test]
    fn test_settings_field_config_parsing() {
        let settings_src = r#"@settings(format:json){
          {
            "meta": {
              "aliases": {
                "type": "list",
                "always_newline": true
              },
              "created": {
                "type": "datetime",
                "format": "rfc3339"
              },
              "modified": {
                "type": "datetime",
                "format": "rfc3339"
              }
            }
          }
        }"#;
        let cfg = load_config_from_str(settings_src).unwrap();
        assert_eq!(cfg.meta_fields.get("aliases").unwrap().always_newline, true);
        assert_eq!(cfg.meta_fields.get("created").unwrap().format.as_deref(), Some("rfc3339"));
        assert_eq!(cfg.meta_fields.get("modified").unwrap().format.as_deref(), Some("rfc3339"));
    }

    #[test]
    fn test_rfc3339_and_aliases_always_newline_meta_field_formatting() {
        let md = "---\naliases:\n  - rust\n  - typedmark\ncreated: 2026-06-17T05:52:44\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = typedmark_markdown::from_markdown(md);

        let mut meta_fields = std::collections::BTreeMap::new();
        meta_fields.insert("aliases".to_string(), FieldConfig {
            field_type: Some("list".to_string()),
            always_newline: true,
            ..Default::default()
        });
        meta_fields.insert("created".to_string(), FieldConfig {
            field_type: Some("datetime".to_string()),
            format: Some("rfc3339".to_string()),
            ..Default::default()
        });
        meta_fields.insert("modified".to_string(), FieldConfig {
            field_type: Some("datetime".to_string()),
            format: Some("rfc3339".to_string()),
            ..Default::default()
        });

        let cfg = PrinterConfig {
            meta_format: Some("yaml".to_string()),
            meta_fields,
            ..Default::default()
        };

        let printed = document_to_tm_with_config(&doc, &cfg);
        assert!(printed.contains("created: 2026-06-17T05:52:44Z"));
        assert!(printed.contains("modified: 2026-06-17T05:52:44Z"));
        assert!(printed.contains("aliases:\n    - rust\n    - typedmark"));
    }

    #[test]
    fn test_load_config_from_file_default_config_tm() {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let path = loop {
            let candidate = dir.join("docs/default.config.tm");
            if candidate.exists() {
                break candidate;
            }
            if !dir.pop() {
                panic!("could not find docs/default.config.tm in parent directories");
            }
        };
        let cfg = load_config_from_file(&path).expect("failed to load docs/default.config.tm");
        assert_eq!(cfg.meta_format.as_deref(), Some("yaml"));
        assert_eq!(cfg.meta_always_newline, true);
        assert_eq!(cfg.meta_fields.get("aliases").unwrap().always_newline, true);
        assert_eq!(cfg.meta_fields.get("created").unwrap().format.as_deref(), Some("rfc3339"));
        assert_eq!(cfg.meta_fields.get("created").unwrap().offset.as_deref(), Some("+09:00"));
        assert_eq!(cfg.wikilink_no_space, true);
    }

    #[test]
    fn test_rfc3339_with_offset_formatting() {
        let md = "---\ncreated: 2026-06-17T05:52:44\n---\n\n# Document\n";
        let doc = typedmark_markdown::from_markdown(md);

        let mut meta_fields = std::collections::BTreeMap::new();
        meta_fields.insert("created".to_string(), FieldConfig {
            field_type: Some("datetime".to_string()),
            format: Some("rfc3339".to_string()),
            offset: Some("+09:00".to_string()),
            ..Default::default()
        });

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
        let doc = typedmark_markdown::from_markdown(md);
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
        let doc = typedmark_markdown::from_markdown(md);

        let cfg_default = PrinterConfig::default();
        let printed_default = document_to_tm_with_config(&doc, &cfg_default);
        assert!(printed_default.contains("@(wiki: target)"));
        assert!(printed_default.contains("@[display](wiki: target)"));

        let cfg_no_space = PrinterConfig {
            wikilink_no_space: true,
            ..Default::default()
        };
        let printed_no_space = document_to_tm_with_config(&doc, &cfg_no_space);
        assert!(printed_no_space.contains("@(wiki:target)"));
        assert!(printed_no_space.contains("@[display](wiki:target)"));
    }
}
