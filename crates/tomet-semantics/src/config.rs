use std::collections::HashMap;
use tomet_ast::{Block, Document, Element, ElementValue, Value};

use crate::{ElementKind, classify, normalized_element_args};

/// Supported target format for document exports.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExportType {
    CommonMark,
    Html,
    Custom(String),
}

impl ExportType {
    /// Parse an [`ExportType`] from a format name string (e.g. `"commonmark"`, `"html"`).
    pub fn parse(s: &str) -> Self {
        let trimmed = s.trim();
        match trimmed.to_lowercase().as_str() {
            "commonmark" | "markdown" | "md" => ExportType::CommonMark,
            "html" | "htm" => ExportType::Html,
            other => ExportType::Custom(other.to_string()),
        }
    }

    /// Return the canonical string identifier for this export type.
    pub fn as_str(&self) -> &str {
        match self {
            ExportType::CommonMark => "commonmark",
            ExportType::Html => "html",
            ExportType::Custom(s) => s.as_str(),
        }
    }
}

/// Extracted document configuration settings from top-level `@config` elements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentConfig {
    /// Default format for embedded `{value}` blocks (`json`, `yaml`, `toml`, etc.).
    pub format: Option<String>,
    /// Target export types specified by `export.type` (or `export_type`).
    pub export_type: Vec<ExportType>,
    /// Default output path specified by `export.path` (or `export_path`) scalar string.
    pub export_path: Option<String>,
    /// Per-export-type output paths specified by `export.path` map or format keys.
    pub export_paths: HashMap<ExportType, String>,
    /// Formatting/export style specified by `style`.
    pub style: Option<String>,
    /// Table column width alignment setting (`table.adjust_width`).
    pub table_adjust_width: bool,
    /// Table column width adjustment mode string ("true", "false", "auto").
    pub table_adjust_width_mode: Option<String>,
    /// Table max column width threshold for alignment when auto mode is enabled.
    pub table_max_col_width: Option<usize>,
    /// Table default alignment ("left", "center", "right").
    pub table_align: Option<String>,
    /// User-defined macro templates specified by `macros` map in `@config`.
    pub macros: HashMap<String, String>,
    /// Raw key-value entries collected from `@config` element args and value groups.
    pub entries: Vec<(String, Value)>,
}

impl DocumentConfig {
    /// Returns the target output path for a specific [`ExportType`], falling back to [`export_path`](Self::export_path).
    pub fn export_path_for(&self, target: &ExportType) -> Option<&str> {
        if let Some(path) = self.export_paths.get(target) {
            return Some(path.as_str());
        }
        self.export_path.as_deref()
    }
}

/// Returns this document's `@config` and `@settings` settings merged from all configuration elements in the document.
pub fn document_config(doc: &Document) -> DocumentConfig {
    let mut config = DocumentConfig::default();

    for block in &doc.blocks {
        if let Block::Element(el) = block {
            let kind = classify(el);
            if kind == ElementKind::Config || kind.as_str() == "settings" {
                extract_config_from_element(el, &mut config);
            }
        }
    }

    // Deduplicate export_type list while preserving order
    let mut seen = std::collections::HashSet::new();
    config.export_type.retain(|item| seen.insert(item.clone()));

    config
}

fn extract_config_from_element(el: &Element, config: &mut DocumentConfig) {
    // 1. Process `(args)` if present
    if let Some(Value::Map(args_entries)) = normalized_element_args(el) {
        for (k, v) in args_entries {
            process_config_entry(&k, &v, config);
        }
    }

    // 2. Process `{value}` if present and is a Value::Map
    if let Some(ElementValue::Data(Value::Map(val_entries))) = &el.value {
        for (k, v) in val_entries {
            process_config_entry(k, v, config);
        }
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::String(s) => matches!(s.trim().to_lowercase().as_str(), "true" | "1" | "yes" | "auto"),
        Value::Int(i) => *i != 0,
        _ => false,
    }
}

fn process_config_entry(k: &str, v: &Value, config: &mut DocumentConfig) {
    config.entries.push((k.to_string(), v.clone()));

    match k {
        "format" => {
            if let Some(s) = v.as_str() {
                config.format = Some(s.to_string());
            } else if let Some(table) = v.get("table") {
                if let Some(adjust_width) = table.get("adjust_width") {
                    config.table_adjust_width = is_truthy(adjust_width);
                    if let Some(s) = adjust_width.as_str() {
                        config.table_adjust_width_mode = Some(s.to_string());
                    }
                }
                if let Some(n) = table.get("max_col_width").and_then(|m| m.as_i64()) {
                    config.table_max_col_width = Some(n as usize);
                }
                if let Some(s) = table.get("align").and_then(|a| a.as_str()) {
                    config.table_align = Some(s.to_string());
                }
            }
        }
        "style" => {
            if let Some(s) = v.as_str() {
                config.style = Some(s.to_string());
            }
        }
        "table" => {
            if let Some(adjust_width) = v.get("adjust_width") {
                config.table_adjust_width = is_truthy(adjust_width);
                if let Some(s) = adjust_width.as_str() {
                    config.table_adjust_width_mode = Some(s.to_string());
                }
            }
            if let Some(n) = v.get("max_col_width").and_then(|m| m.as_i64()) {
                config.table_max_col_width = Some(n as usize);
            }
            if let Some(s) = v.get("align").and_then(|a| a.as_str()) {
                config.table_align = Some(s.to_string());
            }
        }
        "table.adjust_width" | "table_adjust_width" => {
            config.table_adjust_width = is_truthy(v);
            if let Some(s) = v.as_str() {
                config.table_adjust_width_mode = Some(s.to_string());
            }
        }
        "table.max_col_width" | "table_max_col_width" => {
            if let Some(n) = v.as_i64() {
                config.table_max_col_width = Some(n as usize);
            }
        }
        "table.align" | "table_align" => {
            if let Some(s) = v.as_str() {
                config.table_align = Some(s.to_string());
            }
        }
        "export" => {
            if let Value::Map(export_map) = v {
                for (ek, ev) in export_map {
                    match ek.as_str() {
                        "type" => process_export_type(ev, config),
                        "path" => process_export_path(ev, config),
                        _ => {}
                    }
                }
            }
        }
        "macros" | "macro" => {
            if let Value::Map(macro_entries) = v {
                for (mk, mv) in macro_entries {
                    if let Some(template) = mv.as_str() {
                        config.macros.insert(mk.clone(), template.to_string());
                    }
                }
            }
        }
        // Fallback for flat keys
        "export_type" => process_export_type(v, config),
        "export_path" => process_export_path(v, config),
        _ if k.starts_with("macros.") || k.starts_with("macro.") => {
            let mname = if k.starts_with("macros.") {
                &k["macros.".len()..]
            } else {
                &k["macro.".len()..]
            };
            if let Some(template) = v.as_str() {
                config.macros.insert(mname.to_string(), template.to_string());
            }
        }
        _ if k.starts_with("export_path.") => {
            let format_sub = &k["export_path.".len()..];
            if let Some(path_str) = clean_path_value(v) {
                config
                    .export_paths
                    .insert(ExportType::parse(format_sub), path_str);
            }
        }
        _ => {}
    }
}

fn process_export_type(v: &Value, config: &mut DocumentConfig) {
    match v {
        Value::String(s) => {
            config.export_type.push(ExportType::parse(s));
        }
        Value::Seq(seq) => {
            for item in seq {
                if let Value::String(s) = item {
                    config.export_type.push(ExportType::parse(s));
                }
            }
        }
        _ => {}
    }
}

fn process_export_path(v: &Value, config: &mut DocumentConfig) {
    match v {
        // A map under `export.path` is always a format -> path table
        // (e.g. `{ commonmark: "README.md", html: "index.html" }") -- an
        // export destination is always just a filesystem path, never a
        // "kind of reference" needing a `file:`/`path:`-style key wrapper.
        Value::Map(map) => {
            for (fk, fv) in map {
                if let Some(path_str) = clean_path_value(fv) {
                    config.export_paths.insert(ExportType::parse(fk), path_str);
                }
            }
        }
        _ => {
            if let Some(path_str) = clean_path_value(v) {
                config.export_path = Some(path_str);
            }
        }
    }
}

/// Trims a plain path string. Non-string values (and the old
/// `file(...)`/`path(...)`/`{ file: ... }` wrapper forms) aren't
/// meaningful here and return `None`.
fn clean_path_value(v: &Value) -> Option<String> {
    v.as_str().map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_parser::parse_document;

    #[test]
    fn parses_grouped_export_config_args() {
        let doc = parse_document(
            "@config(\n  format: json\n  export: {\n    type: commonmark\n    path: \"README.md\"\n  }\n)\n",
        )
        .unwrap();

        let config = document_config(&doc);
        assert_eq!(config.format.as_deref(), Some("json"));
        assert_eq!(config.export_type, vec![ExportType::CommonMark]);
        assert_eq!(config.export_path.as_deref(), Some("README.md"));
        assert_eq!(
            config.export_path_for(&ExportType::CommonMark),
            Some("README.md")
        );
    }

    #[test]
    fn parses_grouped_export_config_with_sequence_and_map_paths() {
        let doc = parse_document(
            "@config(\n  export: {\n    type: [commonmark, html]\n    path: {\n      commonmark: \"README.md\"\n      html: \"index.html\"\n    }\n  }\n)\n",
        )
        .unwrap();

        let config = document_config(&doc);
        assert_eq!(
            config.export_type,
            vec![ExportType::CommonMark, ExportType::Html]
        );
        assert_eq!(
            config.export_path_for(&ExportType::CommonMark),
            Some("README.md")
        );
        assert_eq!(
            config.export_path_for(&ExportType::Html),
            Some("index.html")
        );
    }

    #[test]
    fn handles_no_config_element() {
        let doc = parse_document("#[ Hello ]\n").unwrap();
        let config = document_config(&doc);
        assert_eq!(config, DocumentConfig::default());
    }

    #[test]
    fn parses_table_adjust_width_config() {
        let doc = parse_document("@settings(format:json){\n  {\n    \"table\": {\n      \"adjust_width\": \"true\"\n    }\n  }\n}\n").unwrap();
        let config = document_config(&doc);
        assert!(config.table_adjust_width);
    }

    #[test]
    fn parses_macros_config() {
        let doc = parse_document("@config{\n  macros: {\n    gh: \"https://github.com/org/repo/issues/$1\"\n    jira: \"https://jira.org/browse/$1\"\n  }\n}\n").unwrap();
        let config = document_config(&doc);
        assert_eq!(
            config.macros.get("gh").map(|s| s.as_str()),
            Some("https://github.com/org/repo/issues/$1")
        );
        assert_eq!(
            config.macros.get("jira").map(|s| s.as_str()),
            Some("https://jira.org/browse/$1")
        );
    }
}
