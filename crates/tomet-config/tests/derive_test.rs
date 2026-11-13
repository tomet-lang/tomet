extern crate tomet_config as tomet_spec;

use tomet_ast::{Tuple, Value};
use tomet_config::Config;
use tomet_config::all_config_specs;
use tomet_config::schema::{ConfigEntrySpec, ConfigSpecProvider};
use tomet_tree::ValueExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CustomOrder {
    #[default]
    ArgsFirst,
    ContentFirst,
}

fn parse_custom_order(value: &Value) -> Option<CustomOrder> {
    match value.as_str()? {
        "content" => Some(CustomOrder::ContentFirst),
        "args" => Some(CustomOrder::ArgsFirst),
        _ => None,
    }
}

/// Callout styling configuration.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "callout")]
struct CalloutConfig {
    /// Callout box content style.
    #[config(key = "callout.style.content", default = "block")]
    style: Option<String>,
}

/// Root styling configuration.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "format")]
struct FormatConfig {
    /// Space inside brackets for sections.
    #[config(key = "section.space_inside_brackets", default = false)]
    section_space: bool,

    /// Link order rule.
    #[config(
        key = "link.group_order",
        type = "string",
        default = "args",
        parse_with = "parse_custom_order"
    )]
    link_order: CustomOrder,

    /// Callout settings.
    #[config(flatten)]
    callout: CalloutConfig,
}

#[test]
fn test_flatten_and_custom_parser() {
    let spec = FormatConfig::spec();
    assert_eq!(spec.name, "format");
    assert_eq!(spec.entries.len(), 3);

    assert_eq!(
        spec.entries[0],
        ConfigEntrySpec {
            key: "section.space_inside_brackets",
            value_type: "bool",
            default_value: Some("false"),
            description: "Space inside brackets for sections.",
            aliases: &[],
        }
    );

    let root = Value::Tuple(Tuple::from_named([
        (
            "section".to_string(),
            Value::Tuple(Tuple::from_named([(
                "space_inside_brackets".to_string(),
                Value::Bool(true),
            )])),
        ),
        (
            "link".to_string(),
            Value::Tuple(Tuple::from_named([(
                "group_order".to_string(),
                Value::String("content".to_string()),
            )])),
        ),
        (
            "callout".to_string(),
            Value::Tuple(Tuple::from_named([(
                "style".to_string(),
                Value::Tuple(Tuple::from_named([(
                    "content".to_string(),
                    Value::String("quote".to_string()),
                )])),
            )])),
        ),
    ]));

    let cfg = FormatConfig::from_root(&root);
    assert!(cfg.section_space);
    assert_eq!(cfg.link_order, CustomOrder::ContentFirst);
    assert_eq!(cfg.callout.style.as_deref(), Some("quote"));
}

#[test]
fn test_all_config_specs_coverage() {
    let specs = all_config_specs();
    let group_names: Vec<&str> = specs.iter().map(|g| g.name).collect();
    assert_eq!(group_names, vec!["format", "meta", "workspace", "registry"]);

    let format_group = specs.iter().find(|g| g.name == "format").unwrap();
    assert!(
        format_group
            .entries
            .iter()
            .any(|e| e.key == "section.space_inside_brackets")
    );
    assert!(
        format_group
            .entries
            .iter()
            .any(|e| e.key == "element.link.no_space")
    );
    assert!(
        format_group
            .entries
            .iter()
            .any(|e| e.key == "element.table.adjust_width")
    );

    let meta_group = specs.iter().find(|g| g.name == "meta").unwrap();
    assert!(
        meta_group
            .entries
            .iter()
            .any(|e| e.key == "meta.always_newline")
    );
    assert!(
        meta_group
            .entries
            .iter()
            .any(|e| e.key == "meta.<field>.type")
    );

    let workspace_group = specs.iter().find(|g| g.name == "workspace").unwrap();
    assert!(
        workspace_group
            .entries
            .iter()
            .any(|e| e.key == "workspace.ignore")
    );
    assert!(
        workspace_group
            .entries
            .iter()
            .any(|e| e.key == "workspace.unswept")
    );
}
