//! Reading a `+++` fence body as JSON/YAML/TOML.
//!
//! This runs **after** parsing, not during it. The fence captures its body
//! verbatim as `ElementValue::Raw`; whether that opaque string is later
//! read as JSON, YAML or TOML is decided here, from the element's
//! `format:` argument.
//!
//! That ordering is the point. `format:` used to steer the lexer: a
//! `{...}` body was scanned with brace-depth tracking and handed to
//! another parser mid-parse, which meant the element's own arguments could
//! change the shape of the tree -- and it terminated `#meta(format:yaml)`
//! early at the first unquoted `}` inside otherwise legal YAML. A fence
//! has no such failure mode, and `format:` is now pure interpretation.

use crate::error::{Error, Result};
use tomet_ast::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedFormat {
    Json,
    Yaml,
    Toml,
}

impl EmbeddedFormat {
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "json" => Some(EmbeddedFormat::Json),
            "yaml" => Some(EmbeddedFormat::Yaml),
            "toml" => Some(EmbeddedFormat::Toml),
            _ => None,
        }
    }
}

/// Reads a `+++` fence body as `format`.
///
/// Error positions are relative to `raw` itself (line 1 is the fence
/// body's first line); a caller that knows where the fence sits in the
/// document can offset them.
pub fn parse_raw_body(raw: &str, format: EmbeddedFormat) -> Result<Value> {
    match format {
        EmbeddedFormat::Json => {
            let v: serde_json::Value = serde_json::from_str(raw).map_err(|e| json_error(raw, e))?;
            Ok(json_to_value(v))
        }
        EmbeddedFormat::Yaml => {
            let v: serde_yaml::Value = serde_yaml::from_str(raw).map_err(yaml_error)?;
            Ok(yaml_to_value(v))
        }
        EmbeddedFormat::Toml => {
            let v: toml::Value = toml::from_str(raw).map_err(|e| toml_error(raw, e))?;
            Ok(toml_to_value(v))
        }
    }
}

/// Builds an [`Error`] at `offset` bytes into `raw`.
fn raw_err(raw: &str, offset: usize, message: String) -> Error {
    let offset = offset.min(raw.len());
    let before = &raw[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(1, |l| l.chars().count() + 1);
    Error {
        message,
        line,
        column,
        offset,
    }
}

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

fn json_error(raw: &str, e: serde_json::Error) -> Error {
    let offset = offset_for_line_col(raw, e.line(), e.column());
    raw_err(raw, offset, format!("invalid json: {e}"))
}

fn yaml_error(e: serde_yaml::Error) -> Error {
    Error {
        message: format!("invalid yaml: {e}"),
        line: e.location().map_or(1, |l| l.line()),
        column: e.location().map_or(1, |l| l.column()),
        offset: e.location().map_or(0, |l| l.index()),
    }
}

fn toml_error(raw: &str, e: toml::de::Error) -> Error {
    let offset = e.span().map(|s| s.start).unwrap_or(0);
    raw_err(raw, offset, format!("invalid toml: {e}"))
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
/// `serde_tomet::ser`'s `value_to_key_string` this can't fail, since an
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
