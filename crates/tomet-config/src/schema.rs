//! Runtime configuration schema models and trait definitions.

use crate::groups::{FormatConfig, MacrosConfig, MetaConfig, RegistryConfig, WorkspaceConfig};

/// Specification of a single configuration key/entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntrySpec {
    /// Full dot-delimited key path (e.g. `"heading.space_inside_brackets"`).
    pub key: &'static str,
    /// Value type as a human-readable string (e.g. `"bool"`, `"string"`, `"list(string)"`, `"usize"`).
    pub value_type: &'static str,
    /// Default value representation if defined (e.g. `"false"`, `"auto"`).
    pub default_value: Option<&'static str>,
    /// Prose description extracted from `///` doc-comments.
    pub description: &'static str,
    /// Alternative/legacy key aliases (e.g. `&["link.order"]`).
    pub aliases: &'static [&'static str],
}

/// Specification of a configuration group (e.g. `::config.format`, `::config.workspace`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigGroupSpec {
    /// Name of the group (e.g. `"format"`, `"workspace"`).
    pub name: &'static str,
    /// Description of the group extracted from struct `///` doc-comments.
    pub description: &'static str,
    /// List of configuration entries in this group.
    pub entries: Vec<ConfigEntrySpec>,
}

/// Trait implemented by configuration structs (typically via `#[derive(Config)]`)
/// to provide runtime schema metadata.
pub trait ConfigSpecProvider {
    /// Returns the runtime specification metadata for this configuration group.
    fn spec() -> ConfigGroupSpec;
}

/// Returns the runtime specifications of all registered configuration groups.
pub fn all_config_specs() -> Vec<ConfigGroupSpec> {
    vec![
        FormatConfig::spec(),
        MetaConfig::spec(),
        WorkspaceConfig::spec(),
        MacrosConfig::spec(),
        RegistryConfig::spec(),
    ]
}
