//! Metadata styling rules and per-field constraints ([`MetaConfig`], [`FieldConfig`]).

use std::collections::BTreeMap;
use tomet_ast::Value;
use tomet_tree::ValueExt;

use crate::tree::get_path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldConfig {
    pub field_type: Option<String>,
    pub format: Option<String>,
    pub offset: Option<String>,
    pub always_newline: bool,
    pub length: Option<usize>,
    pub prefix: Option<String>,
    pub force: Option<bool>,
    pub overwrite: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetaConfig {
    pub always_newline: bool,
    pub format: Option<String>,
    pub fields: BTreeMap<String, FieldConfig>,
}

impl MetaConfig {
    /// Extracts metadata configuration from a normalized configuration root.
    pub fn from_root(root: &Value) -> Self {
        let mut cfg = Self::default();

        if let Some(b) = get_path(root, &["meta", "always_newline"]).and_then(|v| v.as_bool()) {
            cfg.always_newline = b;
        }
        if let Some(s) = get_path(root, &["meta", "format"]).and_then(|v| v.as_str()) {
            cfg.format = Some(s.to_string());
        }
        if let Some(Value::Map(map)) = get_path(root, &["meta"]) {
            for (k, v) in map {
                if k != "always_newline" && k != "format" && matches!(v, Value::Map(_)) {
                    parse_meta_field_props(k, v, &mut cfg.fields);
                }
            }
        }

        cfg
    }
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
