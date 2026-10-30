//! One config file's contribution to a stacked [`Config`](crate::Config).
//!
//! A vault's configuration is a stack of files -- the vault config, then each folder config on the
//! way down to a document, then the document's own declarations -- and the nearer layer wins, key
//! by key. Each concern is a declaration of its own (`::format`, `::fields`, `::workspace`,
//! `::blueprints`, `::vocabularies`, `::dictionaries`, `::macros`, `::vars`), and a layer keeps what each one said.
//! Merging happens on those values, before they become a [`Config`](crate::Config): a typed group
//! cannot tell a key that was never written from one written with its default, so two typed groups
//! cannot be merged without guessing.

use std::collections::HashMap;

use tomet_ast::{Document, Element, Value};
use tomet_declaration::{DeclKind, VarDecl, document_entries};
use tomet_tree::ValueExt;

/// The settings, name tables, macros and variables one file declares, ready to be stacked.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConfigLayer {
    format: Vec<(String, Value)>,
    fields: Vec<(String, Value)>,
    workspace: Vec<(String, Value)>,
    blueprints: Vec<(String, Value)>,
    vocabularies: Vec<(String, Value)>,
    dictionaries: Vec<(String, Value)>,
    macros: HashMap<String, String>,
    vars: Vec<(String, VarDecl)>,
}

impl ConfigLayer {
    /// What `doc` declares.
    pub fn from_doc(doc: &Document) -> Self {
        Self {
            format: document_entries(doc, DeclKind::Format),
            fields: document_entries(doc, DeclKind::Fields),
            workspace: document_entries(doc, DeclKind::Workspace),
            blueprints: document_entries(doc, DeclKind::Blueprints),
            vocabularies: document_entries(doc, DeclKind::Vocabularies),
            dictionaries: document_entries(doc, DeclKind::Dictionaries),
            macros: tomet_declaration::document_macros(doc)
                .into_iter()
                .collect(),
            vars: tomet_declaration::document_vars(doc),
        }
    }

    /// The entries this layer holds for the map-like declaration `kind`; `None` for a declaration
    /// that is not a setting of the vault.
    fn entries_mut(&mut self, kind: DeclKind) -> Option<&mut Vec<(String, Value)>> {
        match kind {
            DeclKind::Format => Some(&mut self.format),
            DeclKind::Fields => Some(&mut self.fields),
            DeclKind::Workspace => Some(&mut self.workspace),
            DeclKind::Blueprints => Some(&mut self.blueprints),
            DeclKind::Vocabularies => Some(&mut self.vocabularies),
            DeclKind::Dictionaries => Some(&mut self.dictionaries),
            _ => None,
        }
    }

    /// Drops what only the vault config may declare, returning the declarations it dropped.
    pub fn strip_vault_only(&mut self) -> Vec<DeclKind> {
        let mut dropped = Vec::new();
        for kind in DeclKind::ALL.into_iter().filter(|k| k.is_vault_only()) {
            if let Some(entries) = self.entries_mut(kind)
                && !entries.is_empty()
            {
                entries.clear();
                dropped.push(kind);
            }
        }
        dropped
    }

    /// Lays `inner` over `self`: the inner layer wins where both say something.
    ///
    /// Entries are kept in order and merged key by key when the [`Config`](crate::Config)
    /// is read (a map merges, a list or a scalar is replaced); a name table, the macros and the
    /// variables are replaced by name.
    pub(crate) fn push_inner(&mut self, inner: &ConfigLayer) {
        self.format.extend(inner.format.iter().cloned());
        self.fields.extend(inner.fields.iter().cloned());
        self.workspace.extend(inner.workspace.iter().cloned());
        self.blueprints.extend(inner.blueprints.iter().cloned());
        self.vocabularies.extend(inner.vocabularies.iter().cloned());
        self.dictionaries.extend(inner.dictionaries.iter().cloned());
        self.macros.extend(inner.macros.clone());
        for (name, decl) in &inner.vars {
            match self.vars.iter_mut().find(|(held, _)| held == name) {
                Some(slot) => slot.1 = decl.clone(),
                None => self.vars.push((name.clone(), decl.clone())),
            }
        }
    }

    pub(crate) fn format(&self) -> &[(String, Value)] {
        &self.format
    }

    pub(crate) fn fields(&self) -> &[(String, Value)] {
        &self.fields
    }

    pub(crate) fn workspace(&self) -> &[(String, Value)] {
        &self.workspace
    }

    pub(crate) fn macros(&self) -> &HashMap<String, String> {
        &self.macros
    }

    pub(crate) fn vars(&self) -> &[(String, VarDecl)] {
        &self.vars
    }

    /// The registered blueprint files, in the order registered; a name registered twice is the
    /// later one.
    pub(crate) fn blueprint_paths(&self) -> Vec<String> {
        registered_paths(&self.blueprints)
    }

    /// The registered vocabulary files, in the order registered; a name registered twice is the
    /// later one.
    pub(crate) fn vocabulary_paths(&self) -> Vec<String> {
        registered_paths(&self.vocabularies)
    }

    /// The registered dictionary files, in the order registered; a name registered twice is the
    /// later one.
    pub(crate) fn dictionary_paths(&self) -> Vec<String> {
        registered_paths(&self.dictionaries)
    }
}

/// The `(name, path)` pairs a document registers in its `::blueprints`, `::vocabularies` or
/// `::dictionaries` (`kind`), in the order written. An entry that is not a reference to a file is left out.
pub fn registry_entries(doc: &Document, kind: DeclKind) -> Vec<(String, String)> {
    document_entries(doc, kind)
        .into_iter()
        .filter_map(|(name, value)| registered_path(&value).map(|path| (name, path)))
        .collect()
}

/// The paths of a name table of `name: @file(path)`, a later entry for a name replacing the earlier
/// one in place. An entry that is not a reference to a file registers nothing.
fn registered_paths(table: &[(String, Value)]) -> Vec<String> {
    let mut named: Vec<(&str, Option<String>)> = Vec::new();
    for (name, value) in table {
        let path = registered_path(value);
        match named.iter_mut().find(|(held, _)| *held == name) {
            Some(slot) => slot.1 = path,
            None => named.push((name, path)),
        }
    }
    named.into_iter().filter_map(|(_, path)| path).collect()
}

/// The path an entry of a registry names: `@file(path)`, or the path as a plain string.
fn registered_path(value: &Value) -> Option<String> {
    match value {
        Value::Element(el) => file_target(el),
        Value::String(path) => Some(path.clone()),
        _ => None,
    }
}

fn file_target(el: &Element) -> Option<String> {
    if !el.sigil.is_bare_named("file") {
        return None;
    }
    let args = tomet_semantics::normalized_element_args(el)?;
    args.get("target")
        .and_then(|target| target.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    fn layer(src: &str) -> ConfigLayer {
        ConfigLayer::from_doc(&tomet_parser::parse_document(src).expect("parse"))
    }

    #[test]
    fn the_inner_layer_wins_key_by_key() {
        let outer = layer(
            "::format{ table: { max_col_width: 50 }, link: { no_space: true }, meta: { always_newline: true } }\n::macros{ a: \"outer-a\", b: \"outer-b\" }\n",
        );
        let inner = layer("::format{ table: { max_col_width: 80 } }\n::macros{ b: \"inner-b\" }\n");
        let cfg = Config::from_layers([&outer, &inner]);
        // Replaced where the inner layer says something ...
        assert_eq!(cfg.format.table_max_col_width, Some(80));
        // ... and kept where it does not, down to a sibling key of the same map.
        assert!(cfg.format.link_no_space);
        assert!(cfg.meta.always_newline);
        assert_eq!(cfg.macros.get("a").map(String::as_str), Some("outer-a"));
        assert_eq!(cfg.macros.get("b").map(String::as_str), Some("inner-b"));
    }

    #[test]
    fn a_name_table_is_merged_by_name_and_the_later_entry_wins() {
        let outer = layer(
            "::blueprints{ a: @file(a.tmt), b: @file(b.tmt) }\n::fields{ created: { type: datetime } }\n",
        );
        let inner = layer(
            "::blueprints{ b: @file(b2.tmt), c: @file(c.tmt) }\n::fields{ created: { format: rfc3339 } }\n",
        );
        let cfg = Config::from_layers([&outer, &inner]);
        assert_eq!(
            cfg.registry.blueprints,
            vec![
                "a.tmt".to_string(),
                "b2.tmt".to_string(),
                "c.tmt".to_string()
            ]
        );
        let created = cfg.meta.fields.get("created").expect("created");
        assert_eq!(created.field_type.as_deref(), Some("datetime"));
        assert_eq!(created.format.as_deref(), Some("rfc3339"));
    }

    #[test]
    fn a_registry_entry_that_is_not_a_file_registers_nothing() {
        let cfg = Config::from_layers([&layer(
            "::vocabularies{ deck: @file(deck.tmt), odd: 3, link: @link(x) }\n",
        )]);
        assert_eq!(cfg.registry.vocabularies, vec!["deck.tmt".to_string()]);
    }

    #[test]
    fn dictionaries_are_registered_like_vocabularies_and_stacked_by_name() {
        let outer = layer("::dictionaries{ tomet: @file(a.tmt), user: @file(u.tmt) }\n");
        let inner = layer("::dictionaries{ tomet: @file(b.tmt) }\n");
        let cfg = Config::from_layers([&outer, &inner]);
        assert_eq!(
            cfg.registry.dictionaries,
            vec!["b.tmt".to_string(), "u.tmt".to_string()]
        );
        assert!(cfg.registry.vocabularies.is_empty());
    }

    #[test]
    fn the_workspace_is_dropped_from_a_layer_that_may_not_declare_it() {
        let mut l =
            layer("::workspace{ ignore: list(\"a\") }\n::format{ link: { no_space: true } }\n");
        assert_eq!(l.strip_vault_only(), vec![DeclKind::Workspace]);
        assert!(l.strip_vault_only().is_empty());
        let cfg = Config::from_layers([&l]);
        assert!(cfg.workspace.ignore.is_empty());
        assert!(
            cfg.format.link_no_space,
            "the rest of the layer still applies"
        );
    }
}
