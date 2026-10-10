//! Workspace-level configuration ([`WorkspaceConfig`]).

use tomet_ast::Value;

use crate::tree::{collect_paths, get_path};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkspaceConfig {
    /// `workspace.ignore` -- paths that are not part of this vault.
    /// Neither swept nor resolvable: a reference to one is broken,
    /// because as far as this vault is concerned it is not there.
    pub ignore: Vec<String>,
    /// `workspace.unswept` -- paths that are in the vault and left alone.
    /// E.g. `tests/fixtures` frozen test corpus.
    pub unswept: Vec<String>,
}

impl WorkspaceConfig {
    /// Extracts workspace configuration from a normalized configuration root.
    pub fn from_root(root: &Value) -> Self {
        let mut cfg = Self::default();

        if let Some(v) = get_path(root, &["workspace", "ignore"]) {
            collect_paths(v, &mut cfg.ignore);
        }
        if let Some(v) = get_path(root, &["ignore", "files"]) {
            collect_paths(v, &mut cfg.ignore);
        }
        if let Some(v) = get_path(root, &["workspace", "unswept"]) {
            collect_paths(v, &mut cfg.unswept);
        }

        cfg
    }
}
