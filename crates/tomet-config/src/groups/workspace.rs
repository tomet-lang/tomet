//! Workspace-level configuration ([`WorkspaceConfig`]).

use tomet_config_derive::Config;

/// Workspace-level configuration for vault indexing and file discovery.
#[derive(Debug, Clone, Default, PartialEq, Config)]
#[config(group = "workspace")]
pub struct WorkspaceConfig {
    /// Paths that are not part of this vault (neither swept nor resolvable).
    #[config(key = "workspace.ignore", alias = "ignore.files", type = "list(string)")]
    pub ignore: Vec<String>,

    /// Paths that are in the vault and left alone (e.g. `tests/fixtures` frozen test corpus).
    #[config(key = "workspace.unswept", type = "list(string)")]
    pub unswept: Vec<String>,
}
