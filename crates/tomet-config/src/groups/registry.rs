//! Blueprint and vocabulary file registry declarations ([`RegistryConfig`]).

use tomet_config_derive::Config;

/// Blueprint and vocabulary file registry declarations.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "registry")]
pub struct RegistryConfig {
    /// Blueprint files this vault declares, as paths relative to the config's directory.
    #[config(key = "blueprints", type = "list(string)")]
    pub blueprints: Vec<String>,

    /// Vocabulary files this vault declares, each naming itself with `@vocabulary(ns)`.
    #[config(key = "vocabularies", type = "list(string)")]
    pub vocabularies: Vec<String>,
}
