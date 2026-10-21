//! Metadata styling rules and per-field constraints ([`MetaConfig`], [`FieldConfig`]).

use std::collections::BTreeMap;
use tomet_ast::Value;
use tomet_config_derive::Config;
use tomet_tree::ValueExt;

/// Per-field metadata schema and constraint rules.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "meta_field")]
pub struct FieldConfig {
    /// Expected semantic data type of the metadata field (e.g. `string`, `list`, `datetime`).
    #[config(key = "meta.<field>.type", type = "string")]
    pub field_type: Option<String>,

    /// Datetime or string format pattern (e.g. `rfc3339`).
    #[config(key = "meta.<field>.format", type = "string")]
    pub format: Option<String>,

    /// Fixed timezone offset string (e.g. `+09:00`).
    #[config(key = "meta.<field>.offset", type = "string")]
    pub offset: Option<String>,

    /// Force trailing newline formatting for this field in `@meta{ ... }`.
    #[config(key = "meta.<field>.always_newline", default = false)]
    pub always_newline: bool,

    /// Fixed or maximum list length.
    #[config(key = "meta.<field>.length", type = "int")]
    pub length: Option<usize>,

    /// Static string prefix.
    #[config(key = "meta.<field>.prefix", type = "string")]
    pub prefix: Option<String>,

    /// Force presence validation.
    #[config(key = "meta.<field>.force", type = "bool")]
    pub force: Option<bool>,

    /// Allow overwriting during migrations.
    #[config(key = "meta.<field>.overwrite", type = "bool")]
    pub overwrite: Option<bool>,
}

fn parse_meta_fields(value: &Value) -> Option<BTreeMap<String, FieldConfig>> {
    let mut fields = BTreeMap::new();
    if let Value::Map(map) = value {
        for (k, v) in map {
            if k != "always_newline" && matches!(v, Value::Map(_)) {
                parse_meta_field_props(k, v, &mut fields);
            }
        }
    }
    Some(fields)
}

/// Document metadata formatting and schema rules.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "meta")]
pub struct MetaConfig {
    /// Always write metadata keys on separate newlines.
    #[config(key = "meta.always_newline", default = false)]
    pub always_newline: bool,

    /// Per-field schema attributes template for specification generation.
    #[config(flatten)]
    pub field_schema: FieldConfig,

    /// Parsed per-field configuration instances.
    #[config(key = "meta", type = "map", hide, parse_with = "parse_meta_fields")]
    pub fields: BTreeMap<String, FieldConfig>,
}

fn parse_meta_field_props(
    field_name: &str,
    field_val: &Value,
    fields: &mut BTreeMap<String, FieldConfig>,
) {
    if let Value::Map(props) = field_val {
        let mut field_cfg = fields.get(field_name).cloned().unwrap_or_default();
        for (pk, pv) in props {
            match pk.as_str() {
                "type" => field_cfg.field_type = pv.as_str().map(String::from),
                "format" => field_cfg.format = pv.as_str().map(String::from),
                "offset" => field_cfg.offset = pv.as_str().map(String::from),
                "always_newline" => {
                    if let Some(b) = pv.as_bool() {
                        field_cfg.always_newline = b;
                    }
                }
                "length" => {
                    if let Some(n) = pv.as_i64() {
                        if n > 0 {
                            field_cfg.length = Some(n as usize);
                        }
                    } else if let Some(s) = pv.as_str() {
                        field_cfg.length = s.parse::<usize>().ok();
                    }
                }
                "prefix" => field_cfg.prefix = pv.as_str().map(String::from),
                "force" => {
                    if let Some(b) = pv.as_bool() {
                        field_cfg.force = Some(b);
                    } else if let Some(s) = pv.as_str() {
                        field_cfg.force = Some(s == "true" || s == "1");
                    }
                }
                "overwrite" => {
                    if let Some(b) = pv.as_bool() {
                        field_cfg.overwrite = Some(b);
                    } else if let Some(s) = pv.as_str() {
                        field_cfg.overwrite = Some(s == "true" || s == "1");
                    }
                }
                _ => {}
            }
        }
        fields.insert(field_name.to_string(), field_cfg);
    }
}
