//! Any element's `{...}` body is real JSON/YAML/TOML source, handed as-is
//! to the corresponding crate's own parser rather than TypedMark's
//! lightweight `Value` grammar (`crate::value::parse_value_at`), whenever
//! its `(input)` map has a `format` key naming a recognized format (e.g.
//! `@meta(format:json){...}`, but this isn't specific to `@meta` -- any
//! element works the same way). No `format` key, or an unrecognized value,
//! falls back to that lightweight grammar -- see `EmbeddedFormat::from_tag`
//! and its caller in `document.rs`.

use crate::error::Result;
use crate::value::{err, find_matching_delimiter};
use typedmark_ast::Value;
use typedmark_lexar::Cursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmbeddedFormat {
    Json,
    Yaml,
    Toml,
}

impl EmbeddedFormat {
    pub(crate) fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "json" => Some(EmbeddedFormat::Json),
            "yaml" => Some(EmbeddedFormat::Yaml),
            "toml" => Some(EmbeddedFormat::Toml),
            _ => None,
        }
    }
}

/// Consumes a `{...}` group whose raw text is handed as-is to `format`'s
/// own parser. Mirrors `document.rs::parse_value_group`'s brace handling
/// (it's called from the same call site), but the body itself is opaque to
/// TypedMark: only brace depth and quoted strings are tracked, so the real
/// parser sees exactly the source text the user wrote.
pub(crate) fn parse_embedded_format_value(
    cur: &mut Cursor,
    format: EmbeddedFormat,
) -> Result<Value> {
    let group_start = cur.pos();
    if !cur.eat_str("{") {
        return Err(err(cur, cur.pos(), "expected '{'"));
    }
    let body_start = cur.pos();
    let body_end = find_matching_delimiter(cur, '{', '}', group_start)?;
    let raw = &cur.src()[body_start..body_end];
    cur.set_pos(body_end);
    if !cur.eat_str("}") {
        return Err(err(cur, cur.pos(), "expected '}'"));
    }
    parse_raw(cur, format, body_start, raw)
}

fn parse_raw(cur: &Cursor, format: EmbeddedFormat, body_start: usize, raw: &str) -> Result<Value> {
    match format {
        EmbeddedFormat::Json => {
            let v: serde_json::Value =
                serde_json::from_str(raw).map_err(|e| json_error(cur, body_start, raw, e))?;
            Ok(json_to_value(v))
        }
        EmbeddedFormat::Yaml => {
            let v: serde_yaml::Value =
                serde_yaml::from_str(raw).map_err(|e| yaml_error(cur, body_start, e))?;
            Ok(yaml_to_value(v))
        }
        EmbeddedFormat::Toml => {
            let v: toml::Value = toml::from_str(raw).map_err(|e| toml_error(cur, body_start, e))?;
            Ok(toml_to_value(v))
        }
    }
}

/// Byte offset of `raw`'s 1-based `(line, column)` position, where `column`
/// counts characters (not bytes) into that line -- matches how
/// `serde_json::Error` reports position. Approximate for the same reason
/// the brace scanner above is: real embedded-format bodies are expected to
/// be simple, so exactness for pathological multi-byte-heavy input isn't
/// worth the extra complexity.
fn offset_for_line_col(raw: &str, line: usize, column: usize) -> usize {
    let mut offset = 0usize;
    let mut lines = raw.split('\n');
    for _ in 1..line {
        match lines.next() {
            Some(l) => offset += l.len() + 1,
            None => return raw.len(),
        }
    }
    if let Some(l) = lines.next() {
        let col_offset: usize = l
            .chars()
            .take(column.saturating_sub(1))
            .map(|c| c.len_utf8())
            .sum();
        offset += col_offset;
    }
    offset.min(raw.len())
}

fn json_error(
    cur: &Cursor,
    body_start: usize,
    raw: &str,
    e: serde_json::Error,
) -> crate::error::Error {
    let offset = offset_for_line_col(raw, e.line(), e.column());
    err(cur, body_start + offset, format!("invalid json: {e}"))
}

fn yaml_error(cur: &Cursor, body_start: usize, e: serde_yaml::Error) -> crate::error::Error {
    let offset = e.location().map(|l| l.index()).unwrap_or(0);
    err(cur, body_start + offset, format!("invalid yaml: {e}"))
}

fn toml_error(cur: &Cursor, body_start: usize, e: toml::de::Error) -> crate::error::Error {
    let offset = e.span().map(|s| s.start).unwrap_or(0);
    err(cur, body_start + offset, format!("invalid toml: {e}"))
}

fn json_to_value(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => json_number_to_value(&n),
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(items) => {
            Value::Seq(items.into_iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(map) => Value::Map(
            map.into_iter()
                .map(|(k, v)| (k, json_to_value(v)))
                .collect(),
        ),
    }
}

fn json_number_to_value(n: &serde_json::Number) -> Value {
    match n.as_i64() {
        Some(i) => Value::Int(i),
        None => Value::Float(n.as_f64().unwrap_or_default()),
    }
}

fn yaml_to_value(v: serde_yaml::Value) -> Value {
    match v {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => yaml_number_to_value(&n),
        serde_yaml::Value::String(s) => Value::String(s),
        serde_yaml::Value::Sequence(items) => {
            Value::Seq(items.into_iter().map(yaml_to_value).collect())
        }
        serde_yaml::Value::Mapping(map) => Value::Map(
            map.into_iter()
                .map(|(k, v)| (yaml_key_to_string(k), yaml_to_value(v)))
                .collect(),
        ),
        serde_yaml::Value::Tagged(tagged) => yaml_to_value(tagged.value),
    }
}

fn yaml_number_to_value(n: &serde_yaml::Number) -> Value {
    match n.as_i64() {
        Some(i) => Value::Int(i),
        None => Value::Float(n.as_f64().unwrap_or_default()),
    }
}

/// YAML mapping keys are arbitrary `Value`s, not just strings -- unlike
/// `serde_typedmark::ser`'s `value_to_key_string` this can't fail, since an
/// embedded-YAML body isn't going through a typed `Deserialize` that could
/// reject an odd key: any scalar renders to its natural text, and a
/// non-scalar key (a sequence/mapping key, legal but exotic YAML) falls
/// back to its Debug form rather than being rejected.
fn yaml_key_to_string(k: serde_yaml::Value) -> String {
    match k {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        other => format!("{other:?}"),
    }
}

fn toml_to_value(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Int(i),
        toml::Value::Float(f) => Value::Float(f),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(items) => Value::Seq(items.into_iter().map(toml_to_value).collect()),
        toml::Value::Table(table) => Value::Map(
            table
                .into_iter()
                .map(|(k, v)| (k, toml_to_value(v)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_document;
    use typedmark_ast::{Block, ElementValue, Sigil, Value};

    fn meta_value(src: &str) -> ElementValue {
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::At(Some("meta".into())));
                el.value.clone().expect("expected a {value} group")
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }

    #[test]
    fn json_body_parses_via_the_real_json_parser() {
        let v = meta_value(r#"@meta(format:json){ {"key": "value", "n": 1} }"#);
        assert_eq!(
            v,
            ElementValue::Data(Value::Map(vec![
                ("key".into(), Value::String("value".into())),
                ("n".into(), Value::Int(1)),
            ]))
        );
    }

    #[test]
    fn yaml_body_parses_via_the_real_yaml_parser() {
        let v = meta_value("@meta(format:yaml){\n  key: value\n  n: 1\n}\n");
        assert_eq!(
            v,
            ElementValue::Data(Value::Map(vec![
                ("key".into(), Value::String("value".into())),
                ("n".into(), Value::Int(1)),
            ]))
        );
    }

    #[test]
    fn toml_body_parses_via_the_real_toml_parser() {
        let v = meta_value("@meta(format:toml){\n  key = \"value\"\n  n = 1\n}\n");
        assert_eq!(
            v,
            ElementValue::Data(Value::Map(vec![
                ("key".into(), Value::String("value".into())),
                ("n".into(), Value::Int(1)),
            ]))
        );
    }

    #[test]
    fn invalid_json_body_is_a_parse_error() {
        let err = parse_document(r#"@meta(format:json){ {"key": } }"#).unwrap_err();
        assert!(err.message.contains("invalid json"), "{err}");
    }

    #[test]
    fn invalid_yaml_body_is_a_parse_error() {
        let err = parse_document("@meta(format:yaml){ key: [1, 2 }\n").unwrap_err();
        assert!(err.message.contains("invalid yaml"), "{err}");
    }

    #[test]
    fn invalid_toml_body_is_a_parse_error() {
        let err = parse_document("@meta(format:toml){ = invalid }\n").unwrap_err();
        assert!(err.message.contains("invalid toml"), "{err}");
    }

    #[test]
    fn no_format_key_still_uses_the_lightweight_grammar() {
        // No `(...)` group at all -- `el.input` is `None`, not a map with
        // a `format` key. No format-specific quoting needed here either --
        // this would be invalid JSON and invalid TOML (bare `value`),
        // confirming the real parsers aren't in play.
        let v = meta_value("@meta{ key: value }\n");
        assert_eq!(
            v,
            ElementValue::Data(Value::Map(vec![(
                "key".into(),
                Value::String("value".into())
            )]))
        );
    }

    #[test]
    fn an_unrecognized_format_value_falls_back_to_the_lightweight_grammar() {
        let v = meta_value("@meta(format:xml){ key: value }\n");
        assert_eq!(
            v,
            ElementValue::Data(Value::Map(vec![(
                "key".into(),
                Value::String("value".into())
            )]))
        );
    }

    #[test]
    fn a_non_meta_element_gets_the_same_treatment() {
        // The mechanism is keyed purely on `(input)` having a `format` key
        // -- it isn't specific to `@meta` at all.
        let doc = parse_document(r#"<config>(format:json){ {"key": "value"} }"#).unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("config".into()));
                assert_eq!(
                    el.value,
                    Some(ElementValue::Data(Value::Map(vec![(
                        "key".into(),
                        Value::String("value".into())
                    )])))
                );
            }
            other => panic!("expected an element, got {other:?}"),
        }
    }

    #[test]
    fn nested_braces_in_a_json_body_do_not_confuse_the_closing_brace() {
        let v = meta_value(r#"@meta(format:json){ {"a": {"b": 1}} }"#);
        assert_eq!(
            v,
            ElementValue::Data(Value::Map(vec![(
                "a".into(),
                Value::Map(vec![("b".into(), Value::Int(1))])
            )]))
        );
    }

    #[test]
    fn a_closing_brace_inside_a_json_string_does_not_end_the_group() {
        let v = meta_value(r#"@meta(format:json){ {"a": "}"} }"#);
        assert_eq!(
            v,
            ElementValue::Data(Value::Map(vec![("a".into(), Value::String("}".into()))]))
        );
    }
}
