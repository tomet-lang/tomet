//! =[ tomet-config ]
//!
//! Configuration parsing and styling rule definitions ([`Config`], [`FormatConfig`], [`MetaConfig`], [`WorkspaceConfig`]).
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Centralized Configuration Domain)
//!   | `tomet-config` owns configuration data structures and pure parsing
//!   | ([`load_config_from_str`]). Finding and reading a config file off
//!   | disk is `tomet-load-config`'s job, one layer up.
//! - (Root Group Modules)
//!   - (groups) Root configuration groups ([`FormatConfig`], [`MetaConfig`], [`WorkspaceConfig`], [`RegistryConfig`], [`ApiConfig`]).
//!   - (discovery) Configuration parsing and error handling.
//!   - (tree) AST tree normalization and path lookup utilities.

extern crate self as tomet_config;
extern crate self as tomet_spec;

#[doc(hidden)]
pub mod __private {
    pub use crate::tree::{collect_paths, get_path};
    pub use tomet_ast::Value;
    pub use tomet_tree::ValueExt;
}

pub mod discovery;
pub mod groups;
pub mod layer;
pub mod schema;
pub mod tree;

pub use discovery::{ConfigError, load_config_from_str};
pub use groups::{
    FieldConfig, FormatConfig, GroupOrder, MetaConfig, RegistryConfig, WorkspaceConfig, format,
    meta, registry, workspace,
};
pub use layer::{ConfigLayer, registered_by, registry_entries};
pub use schema::{ConfigEntrySpec, ConfigGroupSpec, ConfigSpecProvider, all_config_specs};
pub use tomet_spec_derive::Config;

use tomet_ast::{File, Value};
use tree::merge_entry;

/// Full Tomet document / vault configuration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    pub format: FormatConfig,
    pub meta: MetaConfig,
    pub workspace: WorkspaceConfig,
    /// The macros a vault defines with `::macro`: name to template. Not a
    /// setting, so it is not read from the `::settings`/`::config` tree.
    pub macros: std::collections::HashMap<String, tomet_ast::Value>,
    /// The variables a vault declares with `::var`, in the order written.
    pub vars: Vec<(String, tomet_declaration::VarDecl)>,
    pub registry: RegistryConfig,
}

/// Type alias for backward compatibility with earlier code referencing `PrinterConfig`.
pub type PrinterConfig = Config;

impl Config {
    /// The config one file declares, with nothing under or over it.
    pub fn from_doc<B>(doc: &File<B>) -> Self {
        Self::from_layers([&ConfigLayer::from_doc(doc)])
    }

    /// The config a stack of layers comes to, outermost first: the inner layer
    /// wins, key by key (a map merges; a list or a scalar is replaced), and a
    /// macro, a variable or a registered name is replaced by name. Built once
    /// from the merged value trees, see [`ConfigLayer`].
    pub fn from_layers<'a>(layers: impl IntoIterator<Item = &'a ConfigLayer>) -> Self {
        let mut merged = ConfigLayer::default();
        for layer in layers {
            merged.push_inner(layer);
        }
        Self::from_merged(&merged)
    }

    /// Returns the configured group order for a specific element name (e.g. "link"),
    /// falling back to the global `group_order`.
    pub fn element_group_order(&self, element_name: &str) -> Option<GroupOrder> {
        self.format.element_group_order(element_name)
    }

    /// Reads the typed groups off one value tree assembled from the declarations: `::format` is
    /// the root, the `::field` are its `meta` map (beside the `meta` formatting hints), the
    /// `::workspace` its `workspace` map, and the registries lists of paths.
    fn from_merged(layer: &ConfigLayer) -> Self {
        let mut root = Vec::new();
        for (key, value) in layer.format() {
            merge_entry(&mut root, key, value);
        }
        for (name, definition) in layer.fields() {
            let field = Value::Map(vec![(name.clone(), definition.clone())]);
            merge_entry(&mut root, "meta", &field);
        }
        for (key, value) in layer.workspace() {
            let entry = Value::Map(vec![(key.clone(), value.clone())]);
            merge_entry(&mut root, "workspace", &entry);
        }
        let paths = |paths: Vec<String>| Value::Seq(paths.into_iter().map(Value::String).collect());
        root.push(("blueprints".to_string(), paths(layer.blueprint_paths())));
        root.push(("vocabularies".to_string(), paths(layer.vocabulary_paths())));
        root.push(("dictionaries".to_string(), paths(layer.dictionary_paths())));
        let root = Value::Map(root);

        Self {
            format: FormatConfig::from_root(&root),
            meta: MetaConfig::from_root(&root),
            workspace: WorkspaceConfig::from_root(&root),
            macros: layer.macros().clone(),
            vars: layer.vars().to_vec(),
            registry: RegistryConfig::from_root(&root),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fields_declaration_parsing() {
        let src = r#"::field(
  aliases: {
    type: list
    always_newline: true
  }
  created: {
    type: datetime
    format: rfc3339
  }
  modified: {
    type: datetime
    format: rfc3339
  }
)
"#;
        let cfg = load_config_from_str(src).unwrap();
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
        let live = r#"::format(callout: { style: { content: block } })
"#;
        assert_eq!(
            load_config_from_str(live)
                .unwrap()
                .format
                .callout_content_style,
            Some("block".to_string())
        );

        let retired = r#"::format(elements: { callout: { style: { content: block } } })
"#;
        assert_eq!(
            load_config_from_str(retired)
                .unwrap()
                .format
                .callout_content_style,
            None
        );
    }

    #[test]
    fn test_workspace_ignore_parsing() {
        let src = r#"::workspace(
  ignore: ("00-09 System/01 Apps/obsidian")
)
"#;
        let cfg = load_config_from_str(src).expect("failed to parse the workspace");
        assert_eq!(
            cfg.workspace.ignore,
            vec!["00-09 System/01 Apps/obsidian".to_string()]
        );
    }

    #[test]
    fn test_heading_and_link_spacing_config() {
        let src = r#"::format(
  heading: { space_inside_brackets: true }
  link: { no_space: true }
)
"#;
        let cfg = load_config_from_str(src).expect("failed to parse format");
        assert!(cfg.format.heading_space_inside_brackets);
        assert!(cfg.format.link_no_space);
    }

    #[test]
    fn test_group_order_config() {
        let src1 = r#"::format(
  element: { group_order: "content_first" }
)
"#;
        let cfg1 = load_config_from_str(src1).expect("failed to parse format");
        assert_eq!(cfg1.format.group_order, Some(GroupOrder::ContentFirst));

        let src2 = r#"::format(
  group_order: "[]()"
)
"#;
        let cfg2 = load_config_from_str(src2).expect("failed to parse format");
        assert_eq!(cfg2.format.group_order, Some(GroupOrder::ContentFirst));

        let src3 = r#"::format(
  group_order: "args_first"
)
"#;
        let cfg3 = load_config_from_str(src3).expect("failed to parse format");
        assert_eq!(cfg3.format.group_order, Some(GroupOrder::ArgsFirst));

        let src4 = r#"::format(
  link: { group_order: "content_first" }
)
"#;
        let cfg4 = load_config_from_str(src4).expect("failed to parse format");
        assert_eq!(cfg4.format.link_group_order, Some(GroupOrder::ContentFirst));
        assert_eq!(cfg4.format.group_order, None);
        assert_eq!(
            cfg4.element_group_order("link"),
            Some(GroupOrder::ContentFirst)
        );
        assert_eq!(cfg4.element_group_order("other"), None);

        let src5 = r#"::format(
  element: { group_order: "args_first" }
  link: { group_order: "[]()" }
)
"#;
        let cfg5 = load_config_from_str(src5).expect("failed to parse format");
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
    fn test_dot_notation_variants() {
        let src_deep_dot = r#"::format(
  callout.style.content: "quote"
  list.multiline.style.content: "box"
  table.adjust_width: true
  table.max_col_width: 80
  table.align: "center"
)
"#;
        let cfg = load_config_from_str(src_deep_dot).expect("failed to parse deep dot format");
        assert_eq!(cfg.format.callout_content_style.as_deref(), Some("quote"));
        assert_eq!(
            cfg.format.list_multiline_style_content.as_deref(),
            Some("box")
        );
        assert_eq!(cfg.format.table_adjust_width.as_deref(), Some("true"));
        assert_eq!(cfg.format.table_max_col_width, Some(80));
        assert_eq!(cfg.format.table_align.as_deref(), Some("center"));

        let src_mixed = r#"::format(
  callout: {
    style.content: "custom"
  }
  list.multiline: {
    style.content: "bullet"
  }
)
"#;
        let cfg_mixed = load_config_from_str(src_mixed).expect("failed to parse mixed format");
        assert_eq!(
            cfg_mixed.format.callout_content_style.as_deref(),
            Some("custom")
        );
        assert_eq!(
            cfg_mixed.format.list_multiline_style_content.as_deref(),
            Some("bullet")
        );
    }

    #[test]
    fn test_dot_notation_meta_and_merge() {
        let src = r#"::format(
  meta.always_newline: true
)
::field(
  url.wiki: { type: list, always_newline: true }
  title: { length: 70 }
)
::workspace(
  ignore: ("target", "build")
  unswept: ("tests/fixtures")
)
"#;
        let cfg = load_config_from_str(src).expect("failed to parse meta and workspace");
        assert!(cfg.meta.always_newline);
        assert!(cfg.meta.fields.contains_key("url.wiki"));
        assert!(cfg.meta.fields.get("url.wiki").unwrap().always_newline);
        assert_eq!(cfg.meta.fields.get("title").unwrap().length, Some(70));
        assert_eq!(cfg.workspace.ignore, vec!["target", "build"]);
        assert_eq!(cfg.workspace.unswept, vec!["tests/fixtures"]);
    }

    #[test]
    fn a_dotted_head_declares_what_the_grouped_form_declares() {
        let dotted = load_config_from_str(
            r#"::macro.copyright("(C) 2026")
::format.meta(always_newline: true)
::format.link(no_space: true, group_order: "[]()")
::field.aliases(type: list, always_newline: true)
::field.url.wiki(type: list, always_newline: true)
::field.title(length: 70)[ the title ]
::workspace.ignore("target", "build")
::workspace.unswept("tests/fixtures")
::blueprint.note(@file(note.blueprint.tmt))
::vocabulary.deck(@file(deck.vocabulary.tmt))
::dictionary.tomet(@file(tomet.dict.tmt))
"#,
        )
        .expect("dotted heads");
        let grouped = load_config_from_str(
            r#"::macro(copyright: "(C) 2026")
::format(meta: { always_newline: true }, link: { no_space: true, group_order: "[]()" })
::field(
  aliases: { type: list, always_newline: true }
  url.wiki: { type: list, always_newline: true }
  title: { length: 70 }
)
::workspace(ignore: ("target", "build"), unswept: ("tests/fixtures"))
::blueprint(note: @file(note.blueprint.tmt))
::vocabulary(deck: @file(deck.vocabulary.tmt))
::dictionary(tomet: @file(tomet.dict.tmt))
"#,
        )
        .expect("grouped");
        assert_eq!(dotted.macros, grouped.macros);
        assert_eq!(dotted.format.link_no_space, grouped.format.link_no_space);
        assert!(dotted.meta.always_newline);
        assert_eq!(dotted.meta.fields.len(), grouped.meta.fields.len());
        assert!(dotted.meta.fields.get("url.wiki").unwrap().always_newline);
        assert_eq!(dotted.meta.fields.get("title").unwrap().length, Some(70));
        assert_eq!(dotted.workspace.ignore, vec!["target", "build"]);
        assert_eq!(dotted.workspace.unswept, vec!["tests/fixtures"]);
        assert_eq!(dotted.registry, grouped.registry);
    }

    #[test]
    fn one_positional_argument_of_a_path_is_a_list_of_one_where_a_list_is_read() {
        // `X.X` is `X`: the one argument is the entry, and a list setting reads it as a list of one.
        let cfg = load_config_from_str("::workspace.ignore(\"target\")\n").expect("parses");
        assert_eq!(cfg.workspace.ignore, vec!["target"]);
    }
}
