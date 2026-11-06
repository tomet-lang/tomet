//! Workspace-level configuration ([`WorkspaceConfig`]).

use tomet_spec_derive::Config;

/// The name of a Folder's entry file when the vault does not choose one.
///
/// This is the one place the default is spelled in code; the `default` of the
/// `workspace.folder.entry` setting repeats it for the generated reference only (a test keeps
/// the two equal).
pub const DEFAULT_FOLDER_ENTRY: &str = "self.tmt";

/// Workspace-level configuration for vault indexing and file discovery.
#[derive(Debug, Clone, PartialEq, Config)]
#[config(group = "workspace")]
pub struct WorkspaceConfig {
    /// Paths that are not part of this vault (neither swept nor resolvable).
    #[config(
        key = "workspace.ignore",
        alias = "ignore.files",
        type = "list(string)"
    )]
    pub ignore: Vec<String>,

    /// Paths that are in the vault and left alone (e.g. `tests/fixtures` frozen test corpus).
    #[config(key = "workspace.unswept", type = "list(string)")]
    pub unswept: Vec<String>,

    /// The file name of a Folder's entry file: the file in a directory whose leading `::` declarations are that Folder's own settings, laid over the vault config for the Folder and below. One name for the whole vault, so only the vault config sets it. A bare file name: not empty, no path separator, not `.` or `..`.
    // The name as written. Code reads it through `WorkspaceConfig::folder_entry`, which does not
    // obey a name that is not valid.
    #[config(key = "workspace.folder.entry", type = "string", default = "self.tmt")]
    pub folder_entry: String,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            ignore: Vec::new(),
            unswept: Vec::new(),
            folder_entry: DEFAULT_FOLDER_ENTRY.to_string(),
        }
    }
}

impl WorkspaceConfig {
    /// The file name of a Folder's entry file in this vault.
    ///
    /// A name that is not a bare file name ([`WorkspaceConfig::folder_entry_problem`]) is not
    /// obeyed: [`DEFAULT_FOLDER_ENTRY`] stands in, and `tomet check` reports the vault config
    /// that wrote it.
    pub fn folder_entry(&self) -> &str {
        if self.folder_entry_problem().is_some() {
            DEFAULT_FOLDER_ENTRY
        } else {
            &self.folder_entry
        }
    }

    /// Why the entry-file name as written cannot name a file in every Folder, or `None` when it can.
    pub fn folder_entry_problem(&self) -> Option<&'static str> {
        match self.folder_entry.as_str() {
            "" => Some("the entry file name is empty"),
            "." | ".." => Some("the entry file name must name a file, not a directory"),
            name if name.contains(['/', '\\', '\0']) => {
                Some("the entry file name must be a bare file name, without a path separator")
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConfigSpecProvider, load_config_from_str};

    fn entry(src: &str) -> WorkspaceConfig {
        load_config_from_str(src).expect("parses").workspace
    }

    #[test]
    fn the_entry_defaults_to_self_tmt() {
        assert_eq!(WorkspaceConfig::default().folder_entry(), "self.tmt");
        assert_eq!(
            entry("::format{ link: { no_space: true } }\n").folder_entry(),
            "self.tmt"
        );
        let spec = WorkspaceConfig::spec();
        let setting = spec
            .entries
            .iter()
            .find(|e| e.key == "workspace.folder.entry")
            .expect("documented");
        assert_eq!(setting.default_value, Some(DEFAULT_FOLDER_ENTRY));
    }

    #[test]
    fn the_vault_sets_the_entry_name() {
        let cfg = entry("::workspace{ folder: { entry: index.tmt } }\n");
        assert_eq!(cfg.folder_entry(), "index.tmt");
        assert_eq!(cfg.folder_entry_problem(), None);
        let quoted = entry("::workspace{ folder: { entry: \"my folder.tmt\" } }\n");
        assert_eq!(quoted.folder_entry(), "my folder.tmt");
    }

    #[test]
    fn the_entry_name_lives_beside_the_other_workspace_settings() {
        let cfg = entry("::workspace{ ignore: list(\"x\"), folder: { entry: index.tmt } }\n");
        assert_eq!(cfg.ignore, vec!["x".to_string()]);
        assert_eq!(cfg.folder_entry(), "index.tmt");
    }

    #[test]
    fn only_a_bare_file_name_is_an_entry_name() {
        for bad in [
            "",
            ".",
            "..",
            "a/b.tmt",
            "../self.tmt",
            "/self.tmt",
            "a\\b.tmt",
        ] {
            let cfg = entry(&format!(
                "::workspace{{ folder: {{ entry: {} }} }}\n",
                tomet_literal(bad)
            ));
            assert!(cfg.folder_entry_problem().is_some(), "{bad:?}");
            assert_eq!(
                cfg.folder_entry(),
                "self.tmt",
                "{bad:?} falls back to the default"
            );
        }
    }

    /// `text` as a quoted Tomet string.
    fn tomet_literal(text: &str) -> String {
        format!("\"{}\"", text.replace('\\', "\\\\"))
    }
}
