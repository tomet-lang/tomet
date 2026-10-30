//! One config file's contribution to a stacked [`Config`](crate::Config).
//!
//! A vault's configuration is a stack of files -- the vault config, then
//! each folder config on the way down to a document, then the document's own
//! `::config` -- and the nearer layer wins, key by key. Merging has to happen
//! on the value tree a layer was read as, before it becomes a [`Config`](crate::Config):
//! a typed group cannot tell a key that was never written from one written with
//! its default, so two typed groups cannot be merged without guessing.

use tomet_ast::Document;
use tomet_semantics::DocumentConfig;

use crate::schema::all_config_specs;

/// Config groups only the vault config may set; a folder config or a
/// document that sets one is reported by `tomet check` and the setting has no
/// effect.
pub const VAULT_ONLY_GROUPS: &[&str] = &["workspace"];

/// Whether `key` (a top-level key of a `::config`, dotted or not) belongs to a
/// vault-only group, through the group's own name, the root of any of its
/// entries, or the root of any of their legacy aliases (`ignore.files`).
pub fn is_vault_only_key(key: &str) -> bool {
    let root = key.split('.').next().unwrap_or(key);
    all_config_specs()
        .iter()
        .filter(|group| VAULT_ONLY_GROUPS.contains(&group.name))
        .any(|group| {
            group.name == root
                || group.entries.iter().any(|entry| {
                    std::iter::once(&entry.key)
                        .chain(entry.aliases)
                        .any(|path| path.split('.').next() == Some(root))
                })
        })
}

/// The settings, macros and variables one file declares, ready to be stacked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigLayer {
    pub(crate) config: DocumentConfig,
}

impl ConfigLayer {
    /// What `doc` declares: its `::config` / `::settings`, `::macros` and `::vars`.
    pub fn from_doc(doc: &Document) -> Self {
        Self {
            config: tomet_semantics::document_config(doc),
        }
    }

    /// The keys this layer sets that only the vault config may set.
    pub fn vault_only_keys(&self) -> Vec<String> {
        self.config
            .entries
            .iter()
            .map(|(key, _)| key)
            .filter(|key| is_vault_only_key(key))
            .cloned()
            .collect()
    }

    /// Drops the vault-only keys from this layer, returning what was dropped.
    pub fn strip_vault_only(&mut self) -> Vec<String> {
        let dropped = self.vault_only_keys();
        self.config
            .entries
            .retain(|(key, _)| !is_vault_only_key(key));
        dropped
    }

    /// Lays `inner` over `self`: the inner layer wins where both say something.
    ///
    /// Entries are kept in order and merged key by key when the [`Config`](crate::Config)
    /// is read (a map merges, a list or a scalar is replaced); macros and
    /// variables are replaced by name.
    pub(crate) fn push_inner(&mut self, inner: &ConfigLayer) {
        self.config
            .entries
            .extend(inner.config.entries.iter().cloned());
        self.config.macros.extend(inner.config.macros.clone());
        for (name, decl) in &inner.config.vars {
            match self.config.vars.iter_mut().find(|(held, _)| held == name) {
                Some(slot) => slot.1 = decl.clone(),
                None => self.config.vars.push((name.clone(), decl.clone())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(src: &str) -> ConfigLayer {
        ConfigLayer::from_doc(&tomet_parser::parse_document(src).expect("parse"))
    }

    #[test]
    fn workspace_keys_are_vault_only_in_every_spelling() {
        assert!(is_vault_only_key("workspace"));
        assert!(is_vault_only_key("workspace.ignore"));
        assert!(is_vault_only_key("ignore"));
        assert!(is_vault_only_key("ignore.files"));
        assert!(!is_vault_only_key("format"));
        assert!(!is_vault_only_key("meta"));
        assert!(!is_vault_only_key("blueprints"));
    }

    #[test]
    fn the_inner_layer_wins_key_by_key() {
        let outer = layer(
            "::config{ format: { table: { max_col_width: 50 }, link: { no_space: true } }, meta: { always_newline: true } }\n::macros{ a: \"outer-a\", b: \"outer-b\" }\n",
        );
        let inner = layer(
            "::config{ format: { table: { max_col_width: 80 } } }\n::macros{ b: \"inner-b\" }\n",
        );
        let cfg = crate::Config::from_layers([&outer, &inner]);
        // Replaced where the inner layer says something ...
        assert_eq!(cfg.format.table_max_col_width, Some(80));
        // ... and kept where it does not, down to a sibling key of the same map.
        assert!(cfg.format.link_no_space);
        assert!(cfg.meta.always_newline);
        assert_eq!(cfg.macros.get("a").map(String::as_str), Some("outer-a"));
        assert_eq!(cfg.macros.get("b").map(String::as_str), Some("inner-b"));
    }

    #[test]
    fn a_list_is_replaced_not_concatenated() {
        let outer = layer("::config{ blueprints: list(\"a.tmt\", \"b.tmt\") }\n");
        let inner = layer("::config{ blueprints: list(\"c.tmt\") }\n");
        let cfg = crate::Config::from_layers([&outer, &inner]);
        assert_eq!(cfg.registry.blueprints, vec!["c.tmt".to_string()]);
    }

    #[test]
    fn strip_vault_only_returns_what_it_dropped() {
        let mut l = layer(
            "::config{ workspace: { ignore: list(\"a\") }, format: { link: { no_space: true } } }\n",
        );
        assert_eq!(l.vault_only_keys(), vec!["workspace".to_string()]);
        assert_eq!(l.strip_vault_only(), vec!["workspace".to_string()]);
        assert!(l.vault_only_keys().is_empty());
        assert!(l.config.entries.iter().any(|(key, _)| key == "format"));
    }
}
