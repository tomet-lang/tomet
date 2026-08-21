//! `PrinterConfig` (loaded from a `default.config.tm`/
//! `typedmark.config.tm`, or an `@settings`/`@config` element in a
//! document) and the config-driven formatting choices it controls: meta
//! format (yaml/json/toml), wikilink/link spacing, callout/list style,
//! and per-field `@meta` rules. Split out of `typedmark-printer` because
//! finding/loading this config is a concern shared by every consumer
//! that needs it (`typedmark-tui`, `typedmark-edit`, `typedmark-indexer`),
//! not something specific to serializing a `Document` back to `.tm`
//! text.

use typedmark_ast::{Block, Document, Element, ElementValue, Sigil, Value};

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
pub struct PrinterConfig {
    pub meta_always_newline: bool,
    pub heading_space_inside_brackets: bool,
    pub meta_format: Option<String>,
    pub meta_fields: std::collections::BTreeMap<String, FieldConfig>,
    pub wikilink_no_space: bool,
    pub link_no_space: bool,
    pub ignore_files: Vec<String>,
    pub callout_content_style: Option<String>,
    pub list_multiline_style_content: Option<String>,
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
                if typedmark_semantics::classify(el)
                    == typedmark_semantics::ElementKind::Custom("settings".to_string())
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
                            let mut field_cfg =
                                cfg.meta_fields.get(field_name).cloned().unwrap_or_default();
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
                                } else if pk == "length" {
                                    if let Value::Int(n) = pv {
                                        if *n > 0 {
                                            field_cfg.length = Some(*n as usize);
                                        }
                                    } else if let Value::String(s) = pv {
                                        field_cfg.length = s.parse::<usize>().ok();
                                    }
                                } else if pk == "prefix" {
                                    if let Value::String(s) = pv {
                                        field_cfg.prefix = Some(s.clone());
                                    }
                                } else if pk == "force" {
                                    if let Value::Bool(b) = pv {
                                        field_cfg.force = Some(*b);
                                    } else if let Value::String(s) = pv {
                                        field_cfg.force = Some(s == "true" || s == "1");
                                    }
                                } else if pk == "overwrite" {
                                    if let Value::Bool(b) = pv {
                                        field_cfg.overwrite = Some(*b);
                                    } else if let Value::String(s) = pv {
                                        field_cfg.overwrite = Some(s == "true" || s == "1");
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
            if k == "link" {
                if let Value::Map(props) = v {
                    for (pk, pv) in props {
                        if pk == "no_space" {
                            if let Value::Bool(b) = pv {
                                cfg.link_no_space = *b;
                            } else if let Value::String(s) = pv {
                                cfg.link_no_space = s == "true" || s == "1";
                            }
                        }
                    }
                }
            }
            if k == "callout" {
                if let Value::Map(props) = v {
                    Self::parse_callout_props(props, cfg);
                }
            }
            if k == "list" {
                if let Value::Map(props) = v {
                    Self::parse_list_props(props, cfg);
                }
            }
            if k == "elements" {
                if let Value::Map(elems) = v {
                    for (ek, ev) in elems {
                        if ek == "callout" {
                            if let Value::Map(props) = ev {
                                Self::parse_callout_props(props, cfg);
                            }
                        } else if ek == "list" {
                            if let Value::Map(props) = ev {
                                Self::parse_list_props(props, cfg);
                            }
                        }
                    }
                }
            }
            if k == "ignore" {
                if let Value::Map(props) = v {
                    for (pk, pv) in props {
                        if pk == "files" {
                            if let Value::Seq(items) = pv {
                                for item in items {
                                    if let Value::String(s) = item {
                                        if !cfg.ignore_files.contains(s) {
                                            cfg.ignore_files.push(s.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn parse_callout_props(props: &[(String, Value)], cfg: &mut Self) {
        for (pk, pv) in props {
            if pk == "style" {
                if let Value::Map(sprops) = pv {
                    for (sk, sv) in sprops {
                        if sk == "content" {
                            if let Value::String(s) = sv {
                                cfg.callout_content_style = Some(s.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn parse_list_props(props: &[(String, Value)], cfg: &mut Self) {
        for (pk, pv) in props {
            if pk == "multiline" {
                if let Value::Map(mprops) = pv {
                    for (mk, mv) in mprops {
                        if mk == "style" {
                            if let Value::Map(sprops) = mv {
                                for (sk, sv) in sprops {
                                    if sk == "content" {
                                        if let Value::String(s) = sv {
                                            cfg.list_multiline_style_content = Some(s.clone());
                                        }
                                    }
                                }
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
            "link" => {
                if let Value::Map(map) = value {
                    for (lk, lv) in map {
                        if lk == "no_space" {
                            if let Value::Bool(b) = lv {
                                cfg.link_no_space = *b;
                            } else if let Value::String(s) = lv {
                                cfg.link_no_space = s == "true" || s == "1";
                            }
                        }
                    }
                }
            }
            "link.no_space" => {
                if let Value::Bool(b) = value {
                    cfg.link_no_space = *b;
                } else if let Value::String(s) = value {
                    cfg.link_no_space = s == "true" || s == "1";
                }
            }
            "callout" => {
                if let Value::Map(map) = value {
                    Self::parse_callout_props(map, cfg);
                }
            }
            "callout.style.content" => {
                if let Value::String(s) = value {
                    cfg.callout_content_style = Some(s.clone());
                }
            }
            "callout.style" => {
                if let Value::Map(map) = value {
                    for (sk, sv) in map {
                        if sk == "content" {
                            if let Value::String(s) = sv {
                                cfg.callout_content_style = Some(s.clone());
                            }
                        }
                    }
                }
            }
            "list" => {
                if let Value::Map(map) = value {
                    Self::parse_list_props(map, cfg);
                }
            }
            "list.multiline.style.content" => {
                if let Value::String(s) = value {
                    cfg.list_multiline_style_content = Some(s.clone());
                }
            }
            "list.multiline.style" => {
                if let Value::Map(map) = value {
                    for (sk, sv) in map {
                        if sk == "content" {
                            if let Value::String(s) = sv {
                                cfg.list_multiline_style_content = Some(s.clone());
                            }
                        }
                    }
                }
            }
            "list.multiline" => {
                if let Value::Map(map) = value {
                    for (mk, mv) in map {
                        if mk == "style" {
                            if let Value::Map(sprops) = mv {
                                for (sk, sv) in sprops {
                                    if sk == "content" {
                                        if let Value::String(s) = sv {
                                            cfg.list_multiline_style_content = Some(s.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "ignore" => {
                if let Value::Map(map) = value {
                    for (ik, iv) in map {
                        if ik == "files" {
                            if let Value::Seq(items) = iv {
                                for item in items {
                                    if let Value::String(s) = item {
                                        if !cfg.ignore_files.contains(s) {
                                            cfg.ignore_files.push(s.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "ignore.files" => {
                if let Value::Seq(items) = value {
                    for item in items {
                        if let Value::String(s) = item {
                            if !cfg.ignore_files.contains(s) {
                                cfg.ignore_files.push(s.clone());
                            }
                        }
                    }
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

/// Searches `start` and its parent directories for `default.config.tm` or `typedmark.config.tm`.
/// Returns `(PrinterConfig, config_file_path, config_directory_path)`.
pub fn find_config_file(
    start: &std::path::Path,
) -> Option<(PrinterConfig, std::path::PathBuf, std::path::PathBuf)> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let candidates = [
            current.join("default.config.tm"),
            current.join("typedmark.config.tm"),
        ];
        for candidate in candidates {
            if candidate.exists() {
                if let Ok(cfg) = load_config_from_file(&candidate) {
                    return Some((cfg, candidate, current));
                }
            }
        }
        if !current.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            cfg.meta_fields.get("created").unwrap().format.as_deref(),
            Some("rfc3339")
        );
        assert_eq!(
            cfg.meta_fields.get("modified").unwrap().format.as_deref(),
            Some("rfc3339")
        );
    }

    #[test]
    fn test_load_config_from_file_default_config_tm() {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let path = loop {
            let candidate = dir.join("docs/tests/test.config.tm");
            if candidate.exists() {
                break candidate;
            }
            if !dir.pop() {
                panic!("could not find docs/tests/test.config.tm in parent directories");
            }
        };
        let cfg = load_config_from_file(&path).expect("failed to load docs/tests/test.config.tm");
        assert_eq!(cfg.meta_format.as_deref(), Some("yaml"));
        assert_eq!(cfg.meta_always_newline, true);
        assert_eq!(cfg.meta_fields.get("aliases").unwrap().always_newline, true);
        assert_eq!(
            cfg.meta_fields.get("created").unwrap().format.as_deref(),
            Some("rfc3339")
        );
        assert_eq!(
            cfg.meta_fields.get("created").unwrap().offset.as_deref(),
            Some("+09:00")
        );
        assert_eq!(cfg.wikilink_no_space, true);
        assert_eq!(cfg.callout_content_style.as_deref(), Some("block"));
        assert_eq!(cfg.list_multiline_style_content.as_deref(), Some("box"));
        assert_eq!(
            cfg.ignore_files,
            vec!["00-09 System/01 Apps/obsidian".to_string()]
        );

        let default_config_path = path.parent().unwrap().join("default.config.tm");
        let cfg_default = load_config_from_file(&default_config_path)
            .expect("failed to load docs/tests/default.config.tm");
        assert_eq!(cfg_default.callout_content_style.as_deref(), Some("block"));
        assert_eq!(
            cfg_default.list_multiline_style_content.as_deref(),
            Some("box")
        );
    }

    #[test]
    fn test_ignore_files_config_parsing() {
        // `is_path_ignored`'s own matching behavior is covered by
        // `typedmark-indexer`'s tests now -- this only checks that
        // `ignore.files` parses into `PrinterConfig.ignore_files`.
        let settings_src = r#"@settings(format:json){
  {
    "ignore": {
      "files": [
        "00-09 System/01 Apps/obsidian"
      ]
    }
  }
}
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        assert_eq!(
            cfg.ignore_files,
            vec!["00-09 System/01 Apps/obsidian".to_string()]
        );
    }
}
