//! Blueprint, vocabulary and dictionary file registry declarations ([`RegistryConfig`]).

use tomet_spec_derive::Config;

/// Blueprint, vocabulary and dictionary file registry declarations.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "registry")]
pub struct RegistryConfig {
    /// Blueprint files this vault declares, as paths relative to the config's directory.
    #[config(key = "blueprints", type = "list(string)")]
    pub blueprints: Vec<String>,

    /// Vocabulary files this vault declares, each naming itself with `::vocabulary(ns)`.
    #[config(key = "vocabularies", type = "list(string)")]
    pub vocabularies: Vec<String>,

    /// Dictionary files this vault declares, each naming itself with `::dictionary(name)`.
    #[config(key = "dictionaries", type = "list(string)")]
    pub dictionaries: Vec<String>,
}
