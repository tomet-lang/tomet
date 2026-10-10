//! Macro template definitions ([`MacrosConfig`]).

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use tomet_ast::Value;
use tomet_tree::ValueExt;

use crate::tree::get_path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MacrosConfig {
    pub entries: HashMap<String, String>,
}

impl MacrosConfig {
    /// Extracts macro templates from a normalized configuration root.
    pub fn from_root(root: &Value) -> Self {
        let mut entries = HashMap::new();
        for macro_path in [&["macros"][..], &["macro"][..]] {
            if let Some(Value::Map(map)) = get_path(root, macro_path) {
                for (mk, mv) in map {
                    if let Some(template) = mv.as_str() {
                        entries.insert(mk.clone(), template.to_string());
                    }
                }
            }
        }
        Self { entries }
    }
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
