//! Blueprint and vocabulary file registry declarations ([`RegistryConfig`]).

use tomet_ast::Value;

use crate::tree::{collect_paths, get_path};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegistryConfig {
    /// Blueprint files this vault declares, as paths relative to the
    /// config's own directory. Paths only: a blueprint names itself with
    /// `@blueprint(X)`.
    pub blueprints: Vec<String>,
    /// Vocabulary files this vault declares, each naming itself with
    /// `@vocabulary(ns)`.
    pub vocabularies: Vec<String>,
}

impl RegistryConfig {
    /// Extracts declared blueprint and vocabulary paths from a normalized configuration root.
    pub fn from_root(root: &Value) -> Self {
        let mut cfg = Self::default();

        if let Some(bp) = get_path(root, &["blueprints"]) {
            collect_paths(bp, &mut cfg.blueprints);
        }
        if let Some(voc) = get_path(root, &["vocabularies"]) {
            collect_paths(voc, &mut cfg.vocabularies);
        }

        cfg
    }
}
