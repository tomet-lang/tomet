//! Macro template definitions ([`MacrosConfig`]).

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use tomet_ast::Value;
use tomet_config_derive::Config;
use tomet_tree::ValueExt;

fn parse_macros_map(value: &Value) -> Option<HashMap<String, String>> {
    let mut entries = HashMap::new();
    if let Value::Map(map) = value {
        for (mk, mv) in map {
            if let Some(template) = mv.as_str() {
                entries.insert(mk.clone(), template.to_string());
            }
        }
    }
    Some(entries)
}

/// Macro template expansions and URL prefix templates.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "macros")]
pub struct MacrosConfig {
    /// User-defined macro templates (e.g. `${1}` positional parameters).
    #[config(
        key = "macros",
        alias = "macro",
        type = "map(string, string)",
        parse_with = "parse_macros_map"
    )]
    pub entries: HashMap<String, String>,
}

impl Deref for MacrosConfig {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl DerefMut for MacrosConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entries
    }
}

impl<'a> IntoIterator for &'a MacrosConfig {
    type Item = (&'a String, &'a String);
    type IntoIter = std::collections::hash_map::Iter<'a, String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl IntoIterator for MacrosConfig {
    type Item = (String, String);
    type IntoIter = std::collections::hash_map::IntoIter<String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}
