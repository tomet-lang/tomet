use std::collections::HashMap;
use tomet_ast::{Document, Value};
use tomet_tree::ValueExt;

use tomet_declaration::{DeclKind, VarDecl};

/// Supported target format for document exports.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExportType {
    CommonMark,
    Html,
    Typst,
    /// Pandoc's own AST, JSON-encoded -- not a rendering, but the input
    /// `pandoc -f json` takes, and so the route to every format Pandoc
    /// writes.
    Pandoc,
    Custom(String),
}

impl ExportType {
    /// Parse an [`ExportType`] from a format name string (e.g. `"commonmark"`, `"html"`, `"typst"`).
    pub fn parse(s: &str) -> Self {
        let trimmed = s.trim();
        match trimmed.to_lowercase().as_str() {
            "commonmark" | "markdown" | "md" => ExportType::CommonMark,
            "html" | "htm" => ExportType::Html,
            "typst" | "typ" => ExportType::Typst,
            "pandoc" | "json" => ExportType::Pandoc,
            other => ExportType::Custom(other.to_string()),
        }
    }

    /// Return the canonical string identifier for this export type.
    pub fn as_str(&self) -> &str {
        match self {
            ExportType::CommonMark => "commonmark",
            ExportType::Html => "html",
            ExportType::Typst => "typst",
            ExportType::Pandoc => "pandoc",
            ExportType::Custom(s) => s.as_str(),
        }
    }
}

/// What a document declares about itself that is not formatting or a registry: where it is exported
/// (`::export`), and the macros and variables it defines (`::macros`, `::vars`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentConfig {
    /// Target export types specified by `::export`'s `type`.
    pub export_type: Vec<ExportType>,
    /// Default output path specified by `::export`'s `path` as a plain string.
    pub export_path: Option<String>,
    /// Per-export-type output paths specified by `::export`'s `path` as a map of format to path.
    pub export_paths: HashMap<ExportType, String>,
    /// User-defined macro templates, from the `::macros` declaration.
    pub macros: HashMap<String, String>,
    /// The variables declared with `::vars`, in the order written. A later
    /// declaration of a name replaces an earlier one.
    pub vars: Vec<(String, VarDecl)>,
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

/// Returns this document's `::export`, `::macros` and `::vars`.
///
/// `::export` is read in either form, `(type: commonmark, path: "README.md")` or the same keys in
/// `{...}`; a `path` is one path for every type, or a map of format to path.
pub fn document_config(doc: &Document) -> DocumentConfig {
    let mut config = DocumentConfig::default();

    for (key, value) in tomet_declaration::document_entries(doc, DeclKind::Export) {
        match key.as_str() {
            "type" => process_export_type(&value, &mut config),
            "path" => process_export_path(&value, &mut config),
            _ => {}
        }
    }
    config.macros = tomet_declaration::document_macros(doc)
        .into_iter()
        .collect();
    config.vars = tomet_declaration::document_vars(doc);

    // Deduplicate export_type list while preserving order
    let mut seen = std::collections::HashSet::new();
    config.export_type.retain(|item| seen.insert(item.clone()));

    config
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
    fn parses_the_export_declaration_args() {
        let doc =
            parse_document("::export(\n  type: commonmark\n  path: \"README.md\"\n)\n").unwrap();

        let config = document_config(&doc);
        assert_eq!(config.export_type, vec![ExportType::CommonMark]);
        assert_eq!(config.export_path.as_deref(), Some("README.md"));
        assert_eq!(
            config.export_path_for(&ExportType::CommonMark),
            Some("README.md")
        );
    }

    #[test]
    fn parses_the_export_declaration_data_with_sequence_and_map_paths() {
        let doc = parse_document(
            "::export{\n  type: list(commonmark, html)\n  path: {\n    commonmark: \"README.md\"\n    html: \"index.html\"\n  }\n}\n",
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
    fn handles_no_declarations() {
        let doc = parse_document("#[ Hello ]\n").unwrap();
        let config = document_config(&doc);
        assert_eq!(config, DocumentConfig::default());
    }

    #[test]
    fn parses_the_macros_declaration() {
        let doc = parse_document("::macros{\n  gh: \"https://github.com/org/repo/issues/$1\"\n  jira: \"https://jira.org/browse/$1\"\n}\n").unwrap();
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

    #[test]
    fn the_macros_declaration_reads_every_entry() {
        let doc = parse_document("::macros{\n  a: \"1\"\n  b: \"2\"\n}\n").unwrap();
        let config = document_config(&doc);
        assert_eq!(config.macros.len(), 2);
    }
}
