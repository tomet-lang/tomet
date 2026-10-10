//! =[ tomet-config ]
//!
//! Configuration discovery, loading, and styling rule definitions ([`PrinterConfig`], [`FieldConfig`]).
//!
//! =[ Architecture & Responsibilities ]
//!
//! - Centralized Configuration Domain:
//!   `tomet-config` owns configuration data structures and discovery/loading functions
//!   ([`find_config_file`], [`load_config_from_file`], [`load_config_from_str`]).
//!   Split out of `tomet-printer` because discovering and loading project configuration
//!   is a shared concern across the workspace (`tomet-style`, `tomet-field-utils`,
//!   `tomet-formatter`, `tomet-indexer`, `tomet-tui`, and `apps/cli`) without
//!   requiring document serialization dependencies.
//!
//! =[ Core Capabilities ]
//!
//! - **Configuration Loading & Discovery**: Searches parent directories for `default.config.tmt`
//!   or `tomet.config.tmt`.
//! - **Printer Configuration ([`PrinterConfig`])**: Serialization formats (`yaml`, `json`, `toml`),
//!   heading bracket spacing, link spacing, callout/list styling, and `ignore_files` patterns.
//! - **Per-Field Metadata Rules ([`FieldConfig`])**: Rules mapping metadata keys to type,
//!   formatting, offset, prefix, and overwrite constraints.

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
    /// `api.rust.out` -- destination directory for extracted Rust API .tmt documents.
    pub api_rust_out: Option<String>,
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
        let mut normalized_entries = Vec::new();
        for (k, v) in &config.entries {
            if k == "format" {
                if let Value::Map(map) = v {
                    for (sub_k, sub_v) in map {
                        merge_entry(&mut normalized_entries, sub_k, sub_v);
                    }
                }
            } else if let Some(sub_k) = k.strip_prefix("format.") {
                merge_entry(&mut normalized_entries, sub_k, v);
            } else {
                merge_entry(&mut normalized_entries, k, v);
            }
        }

        let root = Value::Map(normalized_entries);
        let mut cfg = PrinterConfig::default();

        // 1. Meta settings
        if let Some(b) = get_path(&root, &["meta", "always_newline"]).and_then(|v| v.as_bool()) {
            cfg.meta_always_newline = b;
        }
        if let Some(s) = get_path(&root, &["meta", "format"]).and_then(|v| v.as_str()) {
            cfg.meta_format = Some(s.to_string());
        }
        if let Some(Value::Map(map)) = get_path(&root, &["meta"]) {
            for (k, v) in map {
                if k != "always_newline" && k != "format" && matches!(v, Value::Map(_)) {
                    Self::parse_meta_field_props(k, v, &mut cfg);
                }
            }
        }

        // 2. Heading
        if let Some(space) =
            get_path(&root, &["heading", "space_inside_brackets"]).and_then(|v| v.as_bool())
        {
            cfg.heading_space_inside_brackets = space;
        }

        // 3. Link and wikilink
        let parse_bool_or_str = |v: &Value| {
            if let Some(b) = v.as_bool() {
                Some(b)
            } else if let Some(s) = v.as_str() {
                Some(s == "true" || s == "1")
            } else {
                None
            }
        };
        if let Some(b) = get_path(&root, &["link", "no_space"])
            .or_else(|| get_path(&root, &["wikilink", "no_space"]))
            .and_then(parse_bool_or_str)
        {
            cfg.link_no_space = b;
        }
        if let Some(order) = get_path(&root, &["link", "group_order"])
            .or_else(|| get_path(&root, &["link", "order"]))
            .or_else(|| get_path(&root, &["wikilink", "group_order"]))
            .or_else(|| get_path(&root, &["wikilink", "order"]))
            .and_then(parse_group_order)
        {
            cfg.link_group_order = Some(order);
        }

        // 4. Element group order
        if let Some(order) = get_path(&root, &["element", "group_order"])
            .or_else(|| get_path(&root, &["element", "order"]))
            .or_else(|| get_path(&root, &["group_order"]))
            .and_then(parse_group_order)
        {
            cfg.group_order = Some(order);
        }

        // 5. Callout
        if let Some(s) = get_path(&root, &["callout", "style", "content"]).and_then(|v| v.as_str())
        {
            cfg.callout_content_style = Some(s.to_string());
        }

        // 6. List
        if let Some(s) =
            get_path(&root, &["list", "multiline", "style", "content"]).and_then(|v| v.as_str())
        {
            cfg.list_multiline_style_content = Some(s.to_string());
        }

        // 7. Table
        if let Some(v) = get_path(&root, &["table", "adjust_width"]) {
            if let Some(b) = v.as_bool() {
                cfg.table_adjust_width = Some(b.to_string());
            } else if let Some(s) = v.as_str() {
                cfg.table_adjust_width = Some(s.to_string());
            }
        }
        if let Some(n) = get_path(&root, &["table", "max_col_width"]).and_then(|v| v.as_i64()) {
            cfg.table_max_col_width = Some(n as usize);
        }
        if let Some(s) = get_path(&root, &["table", "align"]).and_then(|v| v.as_str()) {
            cfg.table_align = Some(s.to_string());
        }

        // 8. Macros
        for macro_path in [&["macros"][..], &["macro"][..]] {
            if let Some(Value::Map(entries)) = get_path(&root, macro_path) {
                for (mk, mv) in entries {
                    if let Some(template) = mv.as_str() {
                        cfg.macros.insert(mk.clone(), template.to_string());
                    }
                }
            }
        }

        // 9. Blueprints & Vocabularies
        if let Some(bp) = get_path(&root, &["blueprints"]) {
            collect_paths(bp, &mut cfg.blueprints);
        }
        if let Some(voc) = get_path(&root, &["vocabularies"]) {
            collect_paths(voc, &mut cfg.vocabularies);
        }

        // 10. API
        if let Some(s) = get_path(&root, &["api", "rust", "out"]).and_then(|v| v.as_str()) {
            cfg.api_rust_out = Some(s.to_string());
        }

        // 11. Workspace ignore & unswept
        if let Some(v) = get_path(&root, &["workspace", "ignore"]) {
            collect_paths(v, &mut cfg.ignore_files);
        }
        if let Some(v) = get_path(&root, &["ignore", "files"]) {
            collect_paths(v, &mut cfg.ignore_files);
        }
        if let Some(v) = get_path(&root, &["workspace", "unswept"]) {
            collect_paths(v, &mut cfg.unswept_files);
        }

        cfg
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
}

fn get_path<'a>(mut current: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for &segment in path {
        current = current.get(segment)?;
    }
    Some(current)
}

fn parse_group_order(value: &Value) -> Option<GroupOrder> {
    match value.as_str()? {
        "content_first" | "content_args" | "[]()" | "content" => Some(GroupOrder::ContentFirst),
        "args_first" | "args_content" | "()[]" | "args" => Some(GroupOrder::ArgsFirst),
        _ => None,
    }
}

fn get_or_insert_map<'a>(
    target: &'a mut Vec<(String, Value)>,
    key: &str,
) -> &'a mut Vec<(String, Value)> {
    if let Some(pos) = target.iter().position(|(k, _)| k == key) {
        if !matches!(target[pos].1, Value::Map(_)) {
            target[pos].1 = Value::Map(Vec::new());
        }
    } else {
        target.push((key.to_string(), Value::Map(Vec::new())));
    }
    let pos = target.iter().position(|(k, _)| k == key).unwrap();
    match &mut target[pos].1 {
        Value::Map(m) => m,
        _ => unreachable!(),
    }
}

fn insert_or_merge_prop(map: &mut Vec<(String, Value)>, key: &str, value: &Value) {
    if let Some((_, existing)) = map.iter_mut().find(|(k, _)| k == key) {
        match (existing, value) {
            (Value::Map(ex_props), Value::Map(new_props)) => {
                for (pk, pv) in new_props {
                    if let Some((_, p)) = ex_props.iter_mut().find(|(k, _)| k == pk) {
                        *p = pv.clone();
                    } else {
                        ex_props.push((pk.clone(), pv.clone()));
                    }
                }
            }
            (ex, val) => {
                *ex = val.clone();
            }
        }
    } else {
        map.push((key.to_string(), value.clone()));
    }
}

fn merge_entry(target: &mut Vec<(String, Value)>, key: &str, value: &Value) {
    if let Some((head, tail)) = key.split_once('.') {
        if head == "meta" || head == "macros" || head == "macro" {
            // For meta and macros, `tail` is a flat key (e.g. field name or macro name)
            // that should not be split further.
            let sub_map = get_or_insert_map(target, head);
            insert_or_merge_prop(sub_map, tail, value);
            return;
        }

        // General dotted key: descend or create sub-map at `head`.
        let sub_map = get_or_insert_map(target, head);
        merge_entry(sub_map, tail, value);
    } else if let Value::Map(incoming_map) = value {
        let existing_map = get_or_insert_map(target, key);
        if key == "meta" || key == "macros" || key == "macro" {
            for (sub_k, sub_v) in incoming_map {
                insert_or_merge_prop(existing_map, sub_k, sub_v);
            }
        } else {
            for (sub_k, sub_v) in incoming_map {
                merge_entry(existing_map, sub_k, sub_v);
            }
        }
    } else if let Some((_, existing)) = target.iter_mut().find(|(k, _)| k == key) {
        *existing = value.clone();
    } else {
        target.push((key.to_string(), value.clone()));
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
    // directories to find the repo root, which is sensitive to how deep this crate is nested.

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

    #[test]
    fn test_api_rust_out_config() {
        let src1 = r#"@config{
  api: {
    rust: {
      out: "docs/api"
    }
  }
}
"#;
        let cfg1 = load_config_from_str(src1).expect("failed to parse config");
        assert_eq!(cfg1.api_rust_out, Some("docs/api".to_string()));

        let src2 = r#"@config{
  api.rust: {
    out: "target/api"
  }
}
"#;
        let cfg2 = load_config_from_str(src2).expect("failed to parse config");
        assert_eq!(cfg2.api_rust_out, Some("target/api".to_string()));
    }

    #[test]
    fn test_dot_notation_variants() {
        // Deep dotted: callout.style.content and list.multiline.style.content
        let src_deep_dot = r#"@config{
  callout.style.content: "quote"
  list.multiline.style.content: "box"
  table.adjust_width: true
  table.max_col_width: 80
  table.align: "center"
  api.rust.out: "target/api"
}
"#;
        let cfg = load_config_from_str(src_deep_dot).expect("failed to parse deep dot config");
        assert_eq!(cfg.callout_content_style.as_deref(), Some("quote"));
        assert_eq!(cfg.list_multiline_style_content.as_deref(), Some("box"));
        assert_eq!(cfg.table_adjust_width.as_deref(), Some("true"));
        assert_eq!(cfg.table_max_col_width, Some(80));
        assert_eq!(cfg.table_align.as_deref(), Some("center"));
        assert_eq!(cfg.api_rust_out.as_deref(), Some("target/api"));

        // Mixed nesting: e.g. callout: { style.content: "quote" }
        let src_mixed = r#"@config{
  callout: {
    style.content: "custom"
  }
  list.multiline: {
    style.content: "bullet"
  }
}
"#;
        let cfg_mixed = load_config_from_str(src_mixed).expect("failed to parse mixed config");
        assert_eq!(cfg_mixed.callout_content_style.as_deref(), Some("custom"));
        assert_eq!(
            cfg_mixed.list_multiline_style_content.as_deref(),
            Some("bullet")
        );

        // Format wrapper with dotted entries
        let src_format = r#"@config{
  format.callout.style.content: "formatted"
  format.table.max_col_width: 100
}
"#;
        let cfg_fmt = load_config_from_str(src_format).expect("failed to parse format config");
        assert_eq!(cfg_fmt.callout_content_style.as_deref(), Some("formatted"));
        assert_eq!(cfg_fmt.table_max_col_width, Some(100));
    }

    #[test]
    fn test_dot_notation_meta_and_merge() {
        let src = r#"@config{
  meta.always_newline: true
  meta.format: "yaml"
  meta: {
    url.wiki: { type: list, always_newline: true }
    title: { length: 70 }
  }
  workspace.ignore: list("target", "build")
  workspace.unswept: list("tests/fixtures")
}
"#;
        let cfg = load_config_from_str(src).expect("failed to parse meta and workspace config");
        assert!(cfg.meta_always_newline);
        assert_eq!(cfg.meta_format.as_deref(), Some("yaml"));
        assert!(cfg.meta_fields.contains_key("url.wiki"));
        assert!(cfg.meta_fields.get("url.wiki").unwrap().always_newline);
        assert_eq!(cfg.meta_fields.get("title").unwrap().length, Some(70));
        assert_eq!(cfg.ignore_files, vec!["target", "build"]);
        assert_eq!(cfg.unswept_files, vec!["tests/fixtures"]);
    }
}
