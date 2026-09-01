//! Applies `tomet_config::PrinterConfig`'s style rules (meta format,
//! link-key spacing (`target`, `@link`/`<embed>`'s one shared key), per-field
//! `@meta` formatting) to a single
//! `tomet_ast::Value` or `Element`, returning `.tmt`-syntax text.
//! Deliberately scoped to single nodes -- no `Document`, no tree
//! position, no recursion into sibling/child elements of other kinds --
//! so both `tomet-printer` (rebuilds a whole `Document`) and
//! `tomet-formatter` (patches specific spans of existing `.tmt` text)
//! can depend on this without a circular dependency (`tomet-printer`
//! already depends on `tomet-formatter`). `render_meta_element`
//! specifically covers only `@meta` elements' actual shape per the
//! grammar (`args` an optional format map, `value` always
//! `ElementValue::from_map(Value::Map(...))`) -- not the general
//! element-rendering logic for every other sigil/kind, which stays in
//! `tomet-printer` since it recurses into inline/child content and
//! is genuinely part of rebuilding a whole document from its AST.

use tomet_ast::{Element, ElementValue, Value};
use tomet_config::{FieldConfig, PrinterConfig};
use tomet_field_utils::is_iso8601;

pub fn render_value(v: &Value) -> String {
    format!("{{{}}}", render_value_inner(v))
}

pub fn render_value_inner(v: &Value) -> String {
    render_value_inner_with_config(v, &PrinterConfig::default())
}

/// Formats a string scalar for Tomet source text, wrapping in `"` and escaping
/// inner quotes and special characters only when necessary.
pub fn quote_scalar_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    write_scalar_string(s, &mut out);
    out
}

/// Appends a safely quoted/escaped string scalar to `out`.
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

pub fn render_value_inner_with_config(v: &Value, config: &PrinterConfig) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => quote_scalar_string(s),
        Value::Seq(items) => {
            let rendered: Vec<_> = items
                .iter()
                .map(|item| render_value_inner_with_config(item, config))
                .collect();
            format!("[{}]", rendered.join(", "))
        }
        Value::Map(entries) => {
            if entries.len() == 1 {
                let key = &entries[0].0;
                if key == "target" {
                    let space = if config.link_no_space { "" } else { " " };
                    format!(
                        "@({key}:{space}{})",
                        render_value_inner_with_config(&entries[0].1, config)
                    )
                } else {
                    let mut parts = Vec::new();
                    for (idx, (k, val)) in entries.iter().enumerate() {
                        let val_str = render_value_inner_with_config(val, config);
                        if idx == 0 && (k == "variant" || k == "lang" || k == "format") {
                            parts.push(val_str);
                        } else {
                            parts.push(format!("{k}: {val_str}"));
                        }
                    }
                    parts.join(", ")
                }
            } else {
                let mut parts = Vec::new();
                for (idx, (k, val)) in entries.iter().enumerate() {
                    let val_str = render_value_inner_with_config(val, config);
                    if idx == 0 && (k == "variant" || k == "lang" || k == "format") {
                        parts.push(val_str);
                    } else {
                        parts.push(format!("{k}: {val_str}"));
                    }
                }
                parts.join(", ")
            }
        }
    }
}

pub fn render_args_with_config(v: &Value, config: &PrinterConfig) -> String {
    if let Value::Map(entries) = v {
        if entries.len() == 1 {
            let key = &entries[0].0;
            if key == "target" {
                let space = if config.link_no_space { "" } else { " " };
                return format!(
                    "{key}:{space}{}",
                    render_value_inner_with_config(&entries[0].1, config)
                );
            }
        }
    }
    render_value_inner_with_config(v, config)
}

fn render_meta_field_value(
    v: &Value,
    field_cfg: Option<&FieldConfig>,
    config: &PrinterConfig,
) -> String {
    if let Value::Null = v {
        return String::new();
    }
    if let Some(cfg) = field_cfg {
        if cfg.format.as_deref() == Some("rfc3339") || cfg.field_type.as_deref() == Some("datetime")
        {
            if let Value::String(s) = v {
                return tomet_field_utils::format_rfc3339(s, cfg.offset.as_deref());
            }
        }
        if cfg.always_newline {
            if let Value::Seq(items) = v {
                if items.is_empty() {
                    return "[]".to_string();
                }
                let mut s = String::new();
                for item in items {
                    s.push_str("\n    - ");
                    s.push_str(&render_value_inner_with_config(item, config));
                }
                return s;
            }
        }
    }
    match v {
        Value::String(s) if is_iso8601(s) => s.clone(),
        _ => render_value_inner_with_config(v, config),
    }
}

/// Renders an `@meta(...){...}` element per `config`'s meta format
/// settings (element-local `format:` arg takes priority over
/// `config.meta_format`; multi-line if `config.meta_always_newline` or a
/// format is set, else a single `@meta{k: v, ...}` line). Scoped to
/// `@meta`'s actual shape -- `value` must be `ElementValue::from_map(Value::Map(...))`,
/// which is all the grammar ever produces for `@meta`; anything else
/// renders as a bare `@meta` (with args, if any).
pub fn render_meta_element(el: &Element, config: &PrinterConfig) -> String {
    let effective_format = el
        .args
        .as_ref()
        .and_then(|args| {
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
        })
        .or_else(|| config.meta_format.clone());

    if config.meta_always_newline || effective_format.is_some() {
        if let Some(entries) = el.value.as_ref().map(|v| v.pairs().collect::<Vec<_>>()) {
            let mut out = if let Some(ref fmt) = effective_format {
                format!("@meta(format:{fmt}){{\n")
            } else {
                String::from("@meta{\n")
            };
            for (k, v) in entries {
                let field_cfg = config.meta_fields.get(k);
                let val_str = render_meta_field_value(v, field_cfg, config);
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

    let mut out = String::from("@meta");
    if let Some(args) = &el.args {
        out.push('(');
        out.push_str(&render_args_with_config(args, config));
        out.push(')');
    }
    if let Some(v) = el.value.as_ref().and_then(|v| v.as_data()) {
        out.push('{');
        out.push_str(&render_value_inner_with_config(&v, config));
        out.push('}');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Sigil;

    #[test]
    fn render_value_inner_quotes_strings_needing_it() {
        assert_eq!(
            render_value_inner(&Value::String("plain".to_string())),
            "plain"
        );
        assert_eq!(
            render_value_inner(&Value::String("has space".to_string())),
            "\"has space\""
        );
        assert_eq!(render_value_inner(&Value::Null), "");
    }

    #[test]
    fn render_args_with_config_handles_target() {
        let cfg = PrinterConfig::default();
        let reference = Value::Map(vec![(
            "target".to_string(),
            Value::String("ref:name".to_string()),
        )]);
        assert_eq!(
            render_args_with_config(&reference, &cfg),
            "target: \"ref:name\""
        );

        let mut cfg_no_space = PrinterConfig::default();
        cfg_no_space.link_no_space = true;
        assert_eq!(
            render_args_with_config(&reference, &cfg_no_space),
            "target:\"ref:name\""
        );
    }

    #[test]
    fn render_meta_element_single_line_by_default() {
        let mut el = tomet_tree::element_new(Sigil::block("meta"));
        el.value = Some(ElementValue::from_map(Value::Map(vec![(
            "id".to_string(),
            Value::String("doc-12345678".to_string()),
        )])));
        let cfg = PrinterConfig::default();
        assert_eq!(render_meta_element(&el, &cfg), "@meta{id: doc-12345678}");
    }

    #[test]
    fn render_meta_element_multiline_when_format_configured() {
        let mut el = tomet_tree::element_new(Sigil::block("meta"));
        el.value = Some(ElementValue::from_map(Value::Map(vec![(
            "id".to_string(),
            Value::String("doc-12345678".to_string()),
        )])));
        let mut cfg = PrinterConfig::default();
        cfg.meta_format = Some("yaml".to_string());
        assert_eq!(
            render_meta_element(&el, &cfg),
            "@meta(format:yaml){\n  id: doc-12345678\n}"
        );
    }

    #[test]
    fn test_quote_scalar_string_escaping_and_keywords() {
        assert_eq!(quote_scalar_string("true"), "\"true\"");
        assert_eq!(quote_scalar_string("false"), "\"false\"");
        assert_eq!(quote_scalar_string("null"), "\"null\"");
        assert_eq!(quote_scalar_string("123"), "\"123\"");
        assert_eq!(quote_scalar_string("45.67"), "\"45.67\"");
        assert_eq!(
            quote_scalar_string("hello \"world\""),
            "\"hello \\\"world\\\"\""
        );
        assert_eq!(quote_scalar_string("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(
            quote_scalar_string("2026-06-17T05:52:44Z"),
            "2026-06-17T05:52:44Z"
        );
    }
}
