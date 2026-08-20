use std::collections::HashMap;
use typedmark_ast::{Block, Document, Element, ElementValue, Value};

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

/// Returns this document's `@config` settings merged from all `@config` elements in the document.
pub fn document_config(doc: &Document) -> DocumentConfig {
    let mut config = DocumentConfig::default();

    for block in &doc.blocks {
        if let Block::Element(el) = block {
            if classify(el) == ElementKind::Config {
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

fn process_config_entry(k: &str, v: &Value, config: &mut DocumentConfig) {
    config.entries.push((k.to_string(), v.clone()));

    match k {
        "format" => {
            if let Value::String(s) = v {
                config.format = Some(s.clone());
            }
        }
        "style" => {
            if let Value::String(s) = v {
                config.style = Some(s.clone());
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
        // Fallback for flat keys
        "export_type" => process_export_type(v, config),
        "export_path" => process_export_path(v, config),
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
        Value::Map(map) => {
            // Check if this map represents path for formats (e.g. commonmark: "README.md")
            // vs a file object (e.g. { file: "README.md" })
            let is_file_obj = map.iter().any(|(fk, _)| fk == "file" || fk == "path");
            if is_file_obj {
                if let Some(path_str) = clean_path_value(v) {
                    config.export_path = Some(path_str);
                }
            } else {
                for (fk, fv) in map {
                    if let Some(path_str) = clean_path_value(fv) {
                        config.export_paths.insert(ExportType::parse(fk), path_str);
                    }
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

/// Unwraps path strings from plain string literals, `file(...)`, `path(...)`, or `{ file: ... }` objects.
fn clean_path_value(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => {
            let trimmed = s.trim();
            if (trimmed.starts_with("file(") || trimmed.starts_with("path("))
                && trimmed.ends_with(')')
            {
                let idx = trimmed.find('(')?;
                Some(trimmed[idx + 1..trimmed.len() - 1].trim().to_string())
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Map(map) => {
            for (k, sub_v) in map {
                if k == "file" || k == "path" || k == "target" || k == "src" {
                    return clean_path_value(sub_v);
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_parser::parse_document;

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
    fn unwraps_file_and_path_function_wrappers() {
        let doc = parse_document(
            "@config(\n  export: {\n    type: commonmark\n    path: \"file(README.md)\"\n  }\n)\n",
        )
        .unwrap();

        let config = document_config(&doc);
        assert_eq!(config.export_path.as_deref(), Some("README.md"));

        let doc2 = parse_document(
            "@config(\n  export: {\n    type: html\n    path: \"path(docs/index.html)\"\n  }\n)\n",
        )
        .unwrap();

        let config2 = document_config(&doc2);
        assert_eq!(config2.export_path.as_deref(), Some("docs/index.html"));
    }

    #[test]
    fn handles_no_config_element() {
        let doc = parse_document("#[ Hello ]\n").unwrap();
        let config = document_config(&doc);
        assert_eq!(config, DocumentConfig::default());
    }
}
