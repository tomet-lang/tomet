//! =[ tomet-config ]
//!
//! Configuration discovery, loading, and styling rule definitions ([`Config`], [`FormatConfig`], [`MetaConfig`], [`WorkspaceConfig`]).
//!
//! =[ Architecture & Responsibilities ]
//!
//! - Centralized Configuration Domain:
//!   `tomet-config` owns configuration data structures and discovery/loading functions
//!   ([`find_config_file`], [`load_config_from_file`], [`load_config_from_str`]).
//!
//! - Root Group Modules:
//!   - [`groups`]: Root configuration groups ([`FormatConfig`], [`MetaConfig`], [`WorkspaceConfig`], [`MacrosConfig`], [`RegistryConfig`], [`ApiConfig`]).
//!   - [`discovery`]: Configuration file discovery, loading, and error handling.
//!   - [`tree`]: AST tree normalization and path lookup utilities.

pub mod discovery;
pub mod groups;
pub mod tree;

pub use discovery::{
    ConfigError, config_for, find_config_file, load_config_from_file, load_config_from_str,
};
pub use groups::{
    ApiConfig, FieldConfig, FormatConfig, GroupOrder, MacrosConfig, MetaConfig, RegistryConfig,
    WorkspaceConfig, api, format, macros, meta, registry, workspace,
};

use tomet_ast::{Document, Value};
use tree::merge_entry;

/// Full Tomet document / vault configuration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    pub format: FormatConfig,
    pub meta: MetaConfig,
    pub workspace: WorkspaceConfig,
    pub macros: MacrosConfig,
    pub registry: RegistryConfig,
    pub api: ApiConfig,
}

/// Type alias for backward compatibility with earlier code referencing `PrinterConfig`.
pub type PrinterConfig = Config;

impl Config {
    pub fn from_doc(doc: &Document) -> Self {
        let config = tomet_semantics::document_config(doc);
        Self::from_document_config(&config)
    }

    /// Returns the configured group order for a specific element name (e.g. "link"),
    /// falling back to the global `group_order`.
    pub fn element_group_order(&self, element_name: &str) -> Option<GroupOrder> {
        self.format.element_group_order(element_name)
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

        Self {
            format: FormatConfig::from_root(&root),
            meta: MetaConfig::from_root(&root),
            workspace: WorkspaceConfig::from_root(&root),
            macros: MacrosConfig::from_root(&root),
            registry: RegistryConfig::from_root(&root),
            api: ApiConfig::from_root(&root),
        }
    }
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
        assert_eq!(cfg.meta.format.as_deref(), Some("yaml"));
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
        assert!(cfg.meta.fields.get("aliases").unwrap().always_newline);
        assert_eq!(
            cfg.meta.fields.get("created").unwrap().format.as_deref(),
            Some("rfc3339")
        );
        assert_eq!(
            cfg.meta.fields.get("modified").unwrap().format.as_deref(),
            Some("rfc3339")
        );
    }

    #[test]
    fn callout_style_is_read_from_format_and_no_longer_from_elements() {
        let live = r#"@config(format:json)+++
{ "format": { "callout": { "style": { "content": "block" } } } }
+++"#;
        assert_eq!(
            load_config_from_str(live)
                .unwrap()
                .format
                .callout_content_style,
            Some("block".to_string())
        );

        let retired = r#"@config(format:json)+++
{ "elements": { "callout": { "style": { "content": "block" } } } }
+++"#;
        assert_eq!(
            load_config_from_str(retired)
                .unwrap()
                .format
                .callout_content_style,
            None
        );
    }

    #[test]
    fn test_ignore_files_config_parsing() {
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
            cfg.workspace.ignore,
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
        assert!(cfg.format.heading_space_inside_brackets);
        assert!(cfg.format.link_no_space);
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
        assert_eq!(cfg1.format.group_order, Some(GroupOrder::ContentFirst));

        let src2 = r#"@config(
  format: {
    group_order: "[]()"
  }
)
"#;
        let cfg2 = load_config_from_str(src2).expect("failed to parse config");
        assert_eq!(cfg2.format.group_order, Some(GroupOrder::ContentFirst));

        let src3 = r#"@config(
  format: {
    group_order: "args_first"
  }
)
"#;
        let cfg3 = load_config_from_str(src3).expect("failed to parse config");
        assert_eq!(cfg3.format.group_order, Some(GroupOrder::ArgsFirst));

        let src4 = r#"@config(
  format: {
    link: { group_order: "content_first" }
  }
)
"#;
        let cfg4 = load_config_from_str(src4).expect("failed to parse config");
        assert_eq!(cfg4.format.link_group_order, Some(GroupOrder::ContentFirst));
        assert_eq!(cfg4.format.group_order, None);
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
        assert_eq!(cfg5.format.group_order, Some(GroupOrder::ArgsFirst));
        assert_eq!(cfg5.format.link_group_order, Some(GroupOrder::ContentFirst));
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
        assert_eq!(cfg1.api.rust_out, Some("docs/api".to_string()));

        let src2 = r#"@config{
  api.rust: {
    out: "target/api"
  }
}
"#;
        let cfg2 = load_config_from_str(src2).expect("failed to parse config");
        assert_eq!(cfg2.api.rust_out, Some("target/api".to_string()));
    }

    #[test]
    fn test_dot_notation_variants() {
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
        assert_eq!(cfg.format.callout_content_style.as_deref(), Some("quote"));
        assert_eq!(
            cfg.format.list_multiline_style_content.as_deref(),
            Some("box")
        );
        assert_eq!(cfg.format.table_adjust_width.as_deref(), Some("true"));
        assert_eq!(cfg.format.table_max_col_width, Some(80));
        assert_eq!(cfg.format.table_align.as_deref(), Some("center"));
        assert_eq!(cfg.api.rust_out.as_deref(), Some("target/api"));

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
        assert_eq!(
            cfg_mixed.format.callout_content_style.as_deref(),
            Some("custom")
        );
        assert_eq!(
            cfg_mixed.format.list_multiline_style_content.as_deref(),
            Some("bullet")
        );

        let src_format = r#"@config{
  format.callout.style.content: "formatted"
  format.table.max_col_width: 100
}
"#;
        let cfg_fmt = load_config_from_str(src_format).expect("failed to parse format config");
        assert_eq!(
            cfg_fmt.format.callout_content_style.as_deref(),
            Some("formatted")
        );
        assert_eq!(cfg_fmt.format.table_max_col_width, Some(100));
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
        assert!(cfg.meta.always_newline);
        assert_eq!(cfg.meta.format.as_deref(), Some("yaml"));
        assert!(cfg.meta.fields.contains_key("url.wiki"));
        assert!(cfg.meta.fields.get("url.wiki").unwrap().always_newline);
        assert_eq!(cfg.meta.fields.get("title").unwrap().length, Some(70));
        assert_eq!(cfg.workspace.ignore, vec!["target", "build"]);
        assert_eq!(cfg.workspace.unswept, vec!["tests/fixtures"]);
    }
}
