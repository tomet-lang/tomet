//! `PrinterConfig` (loaded from a `default.config.tmt`/
//! `tomet.config.tmt`, or an `@settings`/`@config` element in a
//! document) and the config-driven formatting choices it controls: meta
//! format (yaml/json/toml), link-key spacing, callout/list style,
//! and per-field `@meta` rules. Split out of `tomet-printer` because
//! finding/loading this config is a concern shared by every consumer
//! that needs it (`tomet-tui`, `tomet-workspace`, `tomet-indexer`),
//! not something specific to serializing a `Document` back to `.tmt`
//! text.

use tomet_ast::{Document, Value};
use tomet_tree::ValueExt;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupOrder {
    #[default]
    ArgsFirst, // @link(args)[content]{value}
    ContentFirst, // @link[content](args){value}
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrinterConfig {
    pub meta_always_newline: bool,
    pub heading_space_inside_brackets: bool,
    pub meta_format: Option<String>,
    pub meta_fields: std::collections::BTreeMap<String, FieldConfig>,
    pub link_no_space: bool,
    pub link_group_order: Option<GroupOrder>,
    pub group_order: Option<GroupOrder>,
    /// `workspace.ignore` -- paths that are not part of this vault.
    /// Neither swept nor resolvable: a reference to one is broken,
    /// because as far as this vault is concerned it is not there.
    pub ignore_files: Vec<String>,
    /// `workspace.unswept` -- paths that are in the vault and left alone.
    ///
    /// Two intents used to share `ignore`, and only one of them is
    /// "not ours". `tests/fixtures` is committed and is the frozen
    /// corpus: it is skipped because a stale export or a broken link
    /// *there* is the coverage, not because the repository does not
    /// contain it. A document saying so could not say it with `@dir`
    /// while one list served both.
    pub unswept_files: Vec<String>,
    pub callout_content_style: Option<String>,
    pub list_multiline_style_content: Option<String>,
    pub table_adjust_width: Option<String>,
    pub table_max_col_width: Option<usize>,
    pub table_align: Option<String>,
    pub macros: std::collections::HashMap<String, String>,
    /// Blueprint files this vault declares, as paths relative to the
    /// config's own directory. Paths only: a blueprint names itself with
    /// `@blueprint(X)`, so a name -> path map would write the name twice
    /// and the path a third time. Same shape as Cargo's
    /// `[workspace] members`.
    pub blueprints: Vec<String>,
    /// Vocabulary files this vault declares, by the same rule -- each one
    /// names itself with `@vocabulary(ns)`.
    pub vocabularies: Vec<String>,
}

/// Reads a declared list of paths. A single string is accepted as a
/// one-element list, which is the only shorthand here -- it expands to
/// the explicit form rather than restating it.
fn collect_paths(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Seq(items) => {
            for item in items {
                if let Some(path) = item.as_str()
                    && !out.iter().any(|p| p == path)
                {
                    out.push(path.to_string());
                }
            }
        }
        other => {
            if let Some(path) = other.as_str()
                && !out.iter().any(|p| p == path)
            {
                out.push(path.to_string());
            }
        }
    }
}

impl PrinterConfig {
    pub fn from_doc(doc: &Document) -> Self {
        let config = tomet_semantics::document_config(doc);
        Self::from_document_config(&config)
    }

    /// Returns the configured group order for a specific element name (e.g. "link"),
    /// falling back to the global `group_order`.
    pub fn element_group_order(&self, element_name: &str) -> Option<GroupOrder> {
        if (element_name == "link" || element_name == "wikilink")
            && let Some(order) = self.link_group_order
        {
            return Some(order);
        }
        self.group_order
    }

    pub fn from_document_config(config: &tomet_semantics::DocumentConfig) -> Self {
        let mut cfg = PrinterConfig::default();
        for (k, v) in &config.entries {
            Self::apply_entry(&mut cfg, k, v);
        }
        cfg
    }

    fn apply_entry(cfg: &mut Self, key: &str, value: &Value) {
        if key == "format" {
            if let Value::Map(map) = value {
                for (k, v) in map {
                    Self::apply_entry(cfg, k, v);
                }
            }
            return;
        }
        if let Some(sub_k) = key.strip_prefix("format.") {
            Self::apply_entry(cfg, sub_k, value);
            return;
        }

        match key {
            "meta" => {
                if let Value::Map(map) = value {
                    for (k, v) in map {
                        if k == "always_newline" {
                            if let Some(b) = v.as_bool() {
                                cfg.meta_always_newline = b;
                            }
                        } else if k == "format" {
                            if let Some(s) = v.as_str() {
                                cfg.meta_format = Some(s.to_string());
                            }
                        } else if let Value::Map(_) = v {
                            Self::parse_meta_field_props(k, v, cfg);
                        }
                    }
                }
            }
            "meta.always_newline" => {
                if let Some(b) = value.as_bool() {
                    cfg.meta_always_newline = b;
                }
            }
            "meta.format" => {
                if let Some(s) = value.as_str() {
                    cfg.meta_format = Some(s.to_string());
                }
            }
            "heading" => {
                if let Some(space) = value.get("space_inside_brackets").and_then(|v| v.as_bool()) {
                    cfg.heading_space_inside_brackets = space;
                }
            }
            "heading.space_inside_brackets" => {
                if let Some(b) = value.as_bool() {
                    cfg.heading_space_inside_brackets = b;
                }
            }
            "link" | "wikilink" => {
                if let Value::Map(map) = value {
                    for (k, v) in map {
                        if k == "no_space" {
                            if let Some(b) = v.as_bool() {
                                cfg.link_no_space = b;
                            } else if let Some(s) = v.as_str() {
                                cfg.link_no_space = s == "true" || s == "1";
                            }
                        } else if (k == "group_order" || k == "order")
                            && let Some(s) = v.as_str()
                        {
                            cfg.link_group_order = match s {
                                "content_first" | "content_args" | "[]()" | "content" => {
                                    Some(GroupOrder::ContentFirst)
                                }
                                "args_first" | "args_content" | "()[]" | "args" => {
                                    Some(GroupOrder::ArgsFirst)
                                }
                                _ => None,
                            };
                        }
                    }
                } else if let Some(no_space) = value.get("no_space") {
                    if let Some(b) = no_space.as_bool() {
                        cfg.link_no_space = b;
                    } else if let Some(s) = no_space.as_str() {
                        cfg.link_no_space = s == "true" || s == "1";
                    }
                }
            }
            "link.group_order" | "link.order" | "wikilink.group_order" | "wikilink.order" => {
                if let Some(s) = value.as_str() {
                    cfg.link_group_order = match s {
                        "content_first" | "content_args" | "[]()" | "content" => {
                            Some(GroupOrder::ContentFirst)
                        }
                        "args_first" | "args_content" | "()[]" | "args" => {
                            Some(GroupOrder::ArgsFirst)
                        }
                        _ => None,
                    };
                }
            }
            "link.no_space" | "wikilink.no_space" => {
                if let Some(b) = value.as_bool() {
                    cfg.link_no_space = b;
                } else if let Some(s) = value.as_str() {
                    cfg.link_no_space = s == "true" || s == "1";
                }
            }
            "element" => {
                if let Value::Map(map) = value {
                    for (k, v) in map {
                        if (k == "group_order" || k == "order")
                            && let Some(s) = v.as_str()
                        {
                            cfg.group_order = match s {
                                "content_first" | "content_args" | "[]()" | "content" => {
                                    Some(GroupOrder::ContentFirst)
                                }
                                "args_first" | "args_content" | "()[]" | "args" => {
                                    Some(GroupOrder::ArgsFirst)
                                }
                                _ => None,
                            };
                        }
                    }
                }
            }
            "element.group_order" | "element.order" | "group_order" => {
                if let Some(s) = value.as_str() {
                    cfg.group_order = match s {
                        "content_first" | "content_args" | "[]()" | "content" => {
                            Some(GroupOrder::ContentFirst)
                        }
                        "args_first" | "args_content" | "()[]" | "args" => {
                            Some(GroupOrder::ArgsFirst)
                        }
                        _ => None,
                    };
                }
            }
            "callout" => {
                if let Value::Map(map) = value {
                    Self::parse_callout_props(map, cfg);
                }
            }
            "callout.style.content" => {
                if let Some(s) = value.as_str() {
                    cfg.callout_content_style = Some(s.to_string());
                }
            }
            "callout.style" => {
                if let Some(s) = value.get("content").and_then(|c| c.as_str()) {
                    cfg.callout_content_style = Some(s.to_string());
                }
            }
            "list" => {
                if let Value::Map(map) = value {
                    Self::parse_list_props(map, cfg);
                }
            }
            "list.multiline.style.content" => {
                if let Some(s) = value.as_str() {
                    cfg.list_multiline_style_content = Some(s.to_string());
                }
            }
            "list.multiline.style" => {
                if let Some(s) = value.get("content").and_then(|c| c.as_str()) {
                    cfg.list_multiline_style_content = Some(s.to_string());
                }
            }
            "list.multiline" => {
                if let Some(s) = value
                    .get("style")
                    .and_then(|st| st.get("content"))
                    .and_then(|c| c.as_str())
                {
                    cfg.list_multiline_style_content = Some(s.to_string());
                }
            }
            "table" => {
                if let Value::Map(map) = value {
                    Self::parse_table_props(map, cfg);
                }
            }
            "table.adjust_width" => {
                if let Some(b) = value.as_bool() {
                    cfg.table_adjust_width = Some(b.to_string());
                } else if let Some(s) = value.as_str() {
                    cfg.table_adjust_width = Some(s.to_string());
                }
            }
            "table.max_col_width" => {
                if let Some(n) = value.as_i64() {
                    cfg.table_max_col_width = Some(n as usize);
                }
            }
            "table.align" => {
                if let Some(s) = value.as_str() {
                    cfg.table_align = Some(s.to_string());
                }
            }
            "macros" | "macro" => {
                if let Value::Map(entries) = value {
                    for (mk, mv) in entries {
                        if let Some(template) = mv.as_str() {
                            cfg.macros.insert(mk.clone(), template.to_string());
                        }
                    }
                }
            }
            "blueprints" => {
                collect_paths(value, &mut cfg.blueprints);
            }
            "vocabularies" => {
                collect_paths(value, &mut cfg.vocabularies);
            }
            // No `elements` arm. It used to reach `callout`, `list` and
            // `table` here and read their style out -- a fifth spelling of
            // `format.callout.style`, used by no config in this repository.
            // `elements:` now means nothing at all in `@settings`, which is
            // what lets `tomet_validator` reject the whole key instead of
            // half of it.
            "workspace" => {
                if let Some(items) = value.get("ignore").and_then(|f| f.as_seq()) {
                    for item in items {
                        if let Some(s) = item.as_str()
                            && !cfg.ignore_files.contains(&s.to_string())
                        {
                            cfg.ignore_files.push(s.to_string());
                        }
                    }
                }
                if let Some(items) = value.get("unswept").and_then(|f| f.as_seq()) {
                    for item in items {
                        if let Some(s) = item.as_str()
                            && !cfg.unswept_files.contains(&s.to_string())
                        {
                            cfg.unswept_files.push(s.to_string());
                        }
                    }
                }
            }
            "ignore" => {
                if let Some(items) = value.get("files").and_then(|f| f.as_seq()) {
                    for item in items {
                        if let Some(s) = item.as_str()
                            && !cfg.ignore_files.contains(&s.to_string())
                        {
                            cfg.ignore_files.push(s.to_string());
                        }
                    }
                }
            }
            "ignore.files" => {
                if let Some(items) = value.as_seq() {
                    for item in items {
                        if let Some(s) = item.as_str()
                            && !cfg.ignore_files.contains(&s.to_string())
                        {
                            cfg.ignore_files.push(s.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn parse_meta_field_props(field_name: &str, field_val: &Value, cfg: &mut Self) {
        if let Value::Map(props) = field_val {
            let mut field_cfg = cfg.meta_fields.get(field_name).cloned().unwrap_or_default();
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
            cfg.meta_fields.insert(field_name.to_string(), field_cfg);
        }
    }

    fn parse_callout_props(props: &[(String, Value)], cfg: &mut Self) {
        let map = Value::Map(props.to_vec());
        if let Some(s) = map
            .get("style")
            .and_then(|st| st.get("content"))
            .and_then(|c| c.as_str())
        {
            cfg.callout_content_style = Some(s.to_string());
        }
    }

    fn parse_list_props(props: &[(String, Value)], cfg: &mut Self) {
        let map = Value::Map(props.to_vec());
        if let Some(s) = map
            .get("multiline")
            .and_then(|m| m.get("style"))
            .and_then(|st| st.get("content"))
            .and_then(|c| c.as_str())
        {
            cfg.list_multiline_style_content = Some(s.to_string());
        }
    }

    fn parse_table_props(props: &[(String, Value)], cfg: &mut Self) {
        let map = Value::Map(props.to_vec());
        if let Some(adjust_width) = map.get("adjust_width") {
            if let Some(b) = adjust_width.as_bool() {
                cfg.table_adjust_width = Some(b.to_string());
            } else if let Some(s) = adjust_width.as_str() {
                cfg.table_adjust_width = Some(s.to_string());
            }
        }
        if let Some(max_col_width) = map.get("max_col_width")
            && let Some(n) = max_col_width.as_i64()
        {
            cfg.table_max_col_width = Some(n as usize);
        }
        if let Some(align) = map.get("align")
            && let Some(s) = align.as_str()
        {
            cfg.table_align = Some(s.to_string());
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: Option<std::path::PathBuf>,
        source: tomet_parser::Error,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "failed to read config file {}: {source}", path.display())
            }
            ConfigError::Parse {
                path: Some(path),
                source,
            } => {
                write!(
                    f,
                    "failed to parse config file {}: {source}",
                    path.display()
                )
            }
            ConfigError::Parse { path: None, source } => {
                write!(f, "failed to parse config source: {source}")
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::Parse { source, .. } => Some(source),
        }
    }
}

pub fn load_config_from_file(path: &std::path::Path) -> Result<PrinterConfig, ConfigError> {
    let src = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let doc = tomet_parser::parse_document(&src).map_err(|source| ConfigError::Parse {
        path: Some(path.to_path_buf()),
        source,
    })?;
    Ok(PrinterConfig::from_doc(&doc))
}

pub fn load_config_from_str(src: &str) -> Result<PrinterConfig, ConfigError> {
    let doc = tomet_parser::parse_document(src)
        .map_err(|source| ConfigError::Parse { path: None, source })?;
    Ok(PrinterConfig::from_doc(&doc))
}

/// The config that governs `path`, for a tool about to act on `src`.
///
/// The nearest `default.config.tmt`/`tomet.config.tmt` up the tree wins,
/// because that file's position is what says where the vault begins.
/// With no vault to find, the document's own `@config` is used, and
/// failing that the defaults.
///
/// This exists so there is one answer to the question. There were two:
/// the LSP resolved a config before formatting and `tomet format` did
/// not, so a file the editor wrote on save was a file `format --check`
/// rejected -- with `table.adjust_width`, `max_col_width` and
/// `always_newline` all live in this repository's own config, the two
/// disagreed on real documents.
pub fn config_for(path: Option<&std::path::Path>, src: &str) -> PrinterConfig {
    if let Some(path) = path
        && let Some((cfg, _, _)) = find_config_file(path)
    {
        return cfg;
    }
    tomet_parser::parse_document(src)
        .map(|doc| PrinterConfig::from_doc(&doc))
        .unwrap_or_default()
}

/// Searches `start` and its parent directories for `default.config.tmt` or `tomet.config.tmt`.
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
            current.join("default.config.tmt"),
            current.join("tomet.config.tmt"),
            current.join("default.config.tm"),
            current.join("tomet.config.tm"),
        ];
        for candidate in candidates {
            if candidate.exists()
                && let Ok(cfg) = load_config_from_file(&candidate)
            {
                // Popping a relative path bottoms out at `""`, which
                // is where a config in the current directory is found
                // from any relative start. `""` is not a usable root:
                // it is neither a file nor a directory, so walking it
                // yields nothing and every caller that enumerates from
                // it silently gets an empty answer.
                // `tomet check-links docs/README.tmt` reported all
                // twelve of its links broken for exactly this reason --
                // the file set it compared them against was empty.
                let root = if current.as_os_str().is_empty() {
                    std::path::PathBuf::from(".")
                } else {
                    current
                };
                return Some((cfg, candidate, root));
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
        let config_src = r#"@config(format:json)+++
{
  "format": {
    "meta": {
      "format": "yaml"
    }
  }
}

+++"#;
        let cfg = load_config_from_str(config_src).unwrap();
        assert_eq!(cfg.meta_format.as_deref(), Some("yaml"));
    }

    #[test]
    fn test_settings_field_config_parsing() {
        let settings_src = r#"@settings(format:json)+++
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

+++"#;
        let cfg = load_config_from_str(settings_src).unwrap();
        assert!(cfg.meta_fields.get("aliases").unwrap().always_newline);
        assert_eq!(
            cfg.meta_fields.get("created").unwrap().format.as_deref(),
            Some("rfc3339")
        );
        assert_eq!(
            cfg.meta_fields.get("modified").unwrap().format.as_deref(),
            Some("rfc3339")
        );
    }

    /// `format.callout.style.content` is the spelling that is read.
    /// `elements.callout.style.content` said the same thing and no longer
    /// does -- one fact, one place. `tomet_validator` reports the retired
    /// key so the change is not a silent no-op for anyone who wrote it.
    #[test]
    fn callout_style_is_read_from_format_and_no_longer_from_elements() {
        let live = r#"@config(format:json)+++
{ "format": { "callout": { "style": { "content": "block" } } } }
+++"#;
        assert_eq!(
            load_config_from_str(live).unwrap().callout_content_style,
            Some("block".to_string())
        );

        let retired = r#"@config(format:json)+++
{ "elements": { "callout": { "style": { "content": "block" } } } }
+++"#;
        assert_eq!(
            load_config_from_str(retired).unwrap().callout_content_style,
            None
        );
    }

    // Loading the shared `test.config.tmt`/`default.config.tmt` fixtures
    // lives in the `tomet-tests` package now. It used to walk up parent
    // directories to find the repo root, which `crates/README.dirs.tmt`
    // flags as sensitive to how deep this crate is nested.

    #[test]
    fn test_ignore_files_config_parsing() {
        // `is_path_ignored`'s own matching behavior is covered by
        // `tomet-indexer`'s tests now -- this only checks that
        // `ignore.files` parses into `PrinterConfig.ignore_files`.
        let settings_src = r#"@settings(format:json)+++
{
  "ignore": {
    "files": [
      "00-09 System/01 Apps/obsidian"
    ]
  }
}
+++
"#;
        let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
        assert_eq!(
            cfg.ignore_files,
            vec!["00-09 System/01 Apps/obsidian".to_string()]
        );
    }

    #[test]
    fn test_heading_and_link_spacing_config() {
        let src = r#"@config(
  format: {
    heading: { space_inside_brackets: true }
    link: { no_space: true }
  }
)
"#;
        let cfg = load_config_from_str(src).expect("failed to parse config");
        assert!(cfg.heading_space_inside_brackets);
        assert!(cfg.link_no_space);
    }

    #[test]
    fn test_group_order_config() {
        let src1 = r#"@config(
  format: {
    element: { group_order: "content_first" }
  }
)
"#;
        let cfg1 = load_config_from_str(src1).expect("failed to parse config");
        assert_eq!(cfg1.group_order, Some(GroupOrder::ContentFirst));

        let src2 = r#"@config(
  format: {
    group_order: "[]()"
  }
)
"#;
        let cfg2 = load_config_from_str(src2).expect("failed to parse config");
        assert_eq!(cfg2.group_order, Some(GroupOrder::ContentFirst));

        let src3 = r#"@config(
  format: {
    group_order: "args_first"
  }
)
"#;
        let cfg3 = load_config_from_str(src3).expect("failed to parse config");
        assert_eq!(cfg3.group_order, Some(GroupOrder::ArgsFirst));

        let src4 = r#"@config(
  format: {
    link: { group_order: "content_first" }
  }
)
"#;
        let cfg4 = load_config_from_str(src4).expect("failed to parse config");
        assert_eq!(cfg4.link_group_order, Some(GroupOrder::ContentFirst));
        assert_eq!(cfg4.group_order, None);
        assert_eq!(
            cfg4.element_group_order("link"),
            Some(GroupOrder::ContentFirst)
        );
        assert_eq!(cfg4.element_group_order("other"), None);

        let src5 = r#"@config(
  format: {
    element: { group_order: "args_first" }
    link: { group_order: "[]()" }
  }
)
"#;
        let cfg5 = load_config_from_str(src5).expect("failed to parse config");
        assert_eq!(cfg5.group_order, Some(GroupOrder::ArgsFirst));
        assert_eq!(cfg5.link_group_order, Some(GroupOrder::ContentFirst));
        assert_eq!(
            cfg5.element_group_order("link"),
            Some(GroupOrder::ContentFirst)
        );
        assert_eq!(
            cfg5.element_group_order("image"),
            Some(GroupOrder::ArgsFirst)
        );
    }
}
