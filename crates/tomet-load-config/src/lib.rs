//! =[ tomet-load-config ]
//!
//! Finds the config that governs a path, by walking up from it, and
//! reads it. `tomet-config` owns the `Config` type and its pure parsing
//! (`load_config_from_str`); this crate is the filesystem half, split
//! out so `tomet-config` itself can stay layer 3 and disk-free.
//!
//! Shares the `tomet-load` name prefix deliberately: both crates are
//! "read something correctly off disk," one a document, one a config.
//! `tomet-load`'s own `Vault::discover` calls [`find_config_file`]
//! directly rather than duplicating the walk.
//!
//! =[ The three layers ]
//!
//! Configuration is a stack of three kinds of file, outermost first:
//!
//! - (vault) [`VAULT_CONFIG_FILE`]: the vault boundary -- a vault is the
//!   directory holding it -- and the vault-only settings.
//! - (folder) [`FOLDER_CONFIG_FILE`]: applies to its folder and below, stacked
//!   from the vault root down to the document.
//! - (document) the document's own `::config`, applied by [`config_for`].
//!
//! The nearer layer wins, key by key; see [`Config::from_layers`] for the
//! merge. The nearest [`VAULT_CONFIG_FILE`] ends the walk, so an outer vault
//! never leaks into an inner one. A setting that only the vault may make
//! (`tomet_config::VAULT_ONLY_GROUPS`) has no effect in a folder or a document
//! layer; `tomet check` reports it.
//!
//! These two names are the only ones. Every user of them, in code and in
//! tests, refers to the constants here rather than spelling them.

use std::path::{Path, PathBuf};

use tomet_config::{Config, ConfigError, ConfigLayer};

/// The vault config: the file whose directory is the vault.
pub const VAULT_CONFIG_FILE: &str = "vault.config.tmt";

/// A folder config: stacked over the vault config for its folder and below.
pub const FOLDER_CONFIG_FILE: &str = ".config.tmt";

/// The layers governing a path, outermost first, with where the vault is.
struct Stack {
    layers: Vec<ConfigLayer>,
    vault_file: PathBuf,
    vault_root: PathBuf,
}

/// Reads one config file as a layer. A file that cannot be read or parsed
/// contributes nothing: it is still where it is, so it still marks the vault.
fn read_layer(path: &Path) -> ConfigLayer {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|src| tomet_parser::parse_document(&src).ok())
        .map(|doc| ConfigLayer::from_doc(&doc))
        .unwrap_or_default()
}

/// A folder (or document) layer may not set what only the vault sets.
fn read_folder_layer(path: &Path) -> ConfigLayer {
    let mut layer = read_layer(path);
    layer.strip_vault_only();
    layer
}

fn find_stack(start: &Path) -> Option<Stack> {
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    // The directories from `start` up to the vault root, nearest first.
    let mut dirs = Vec::new();
    loop {
        dirs.push(current.clone());
        if current.join(VAULT_CONFIG_FILE).is_file() {
            break;
        }
        if !current.pop() {
            return None;
        }
    }

    let vault_root = dirs.last()?.clone();
    let vault_file = vault_root.join(VAULT_CONFIG_FILE);
    let mut layers = Vec::new();
    for dir in dirs.iter().rev() {
        if *dir == vault_root {
            layers.push(read_layer(&vault_file));
        }
        let folder = dir.join(FOLDER_CONFIG_FILE);
        if folder.is_file() {
            layers.push(read_folder_layer(&folder));
        }
    }
    let vault_root = if vault_root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        vault_root
    };
    Some(Stack {
        layers,
        vault_file,
        vault_root,
    })
}

/// The vault governing `start` and its stacked config: searches `start` and its
/// parent directories for the nearest [`VAULT_CONFIG_FILE`].
///
/// Returns `(Config, vault_config_path, vault_directory_path)`. The `Config` is
/// the vault config with every [`FOLDER_CONFIG_FILE`] between the vault root and
/// `start` laid over it; the document's own `::config` is not part of it (that
/// is [`config_for`]'s).
pub fn find_config_file(start: &Path) -> Option<(Config, PathBuf, PathBuf)> {
    let stack = find_stack(start)?;
    let config = Config::from_layers(&stack.layers);
    Some((config, stack.vault_file, stack.vault_root))
}

/// Reads one config file on its own, with no layer under or over it.
pub fn load_config_from_file(path: &Path) -> Result<Config, ConfigError> {
    let src = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let doc = tomet_parser::parse_document(&src).map_err(|source| ConfigError::Parse {
        path: Some(path.to_path_buf()),
        source,
    })?;
    Ok(Config::from_doc(&doc))
}

/// The config that governs `path`, for a tool about to act on `src`.
///
/// The vault config, then each folder config down to `path`, then the
/// document's own `::config` on top, each layer winning over the ones before
/// it. With no vault to find there are no file layers: the document's own
/// `::config` is used, and failing that the defaults.
pub fn config_for(path: Option<&Path>, src: &str) -> Config {
    let mut layers = path
        .and_then(find_stack)
        .map(|stack| stack.layers)
        .unwrap_or_default();
    if let Ok(doc) = tomet_parser::parse_document(src) {
        let mut own = ConfigLayer::from_doc(&doc);
        own.strip_vault_only();
        layers.push(own);
    }
    Config::from_layers(&layers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fresh directory, removed on drop.
    struct Dir(PathBuf);

    impl Dir {
        fn new(tag: &str) -> Self {
            let unique = format!(
                "tomet_load_config_{tag}_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let dir = std::env::temp_dir().join(unique);
            fs::create_dir_all(&dir).unwrap();
            Dir(dir)
        }

        fn write(&self, rel: &str, text: &str) -> PathBuf {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn no_vault_config_means_no_vault() {
        let dir = Dir::new("none");
        let note = dir.write("a/note.tmt", "");
        assert!(find_config_file(&note).is_none());
    }

    #[test]
    fn the_old_names_are_not_read() {
        let dir = Dir::new("old");
        dir.write(
            "default.config.tmt",
            "::config{ format: { link: { no_space: true } } }\n",
        );
        dir.write(
            "tomet.config.tmt",
            "::config{ format: { link: { no_space: true } } }\n",
        );
        let note = dir.write("note.tmt", "");
        assert!(find_config_file(&note).is_none());
    }

    #[test]
    fn the_vault_is_the_directory_holding_the_vault_config() {
        let dir = Dir::new("root");
        let vault_file = dir.write(VAULT_CONFIG_FILE, "::kind(config)\n");
        let note = dir.write("a/b/note.tmt", "");
        let (_, file, root) = find_config_file(&note).expect("a vault");
        assert_eq!(file, vault_file);
        assert_eq!(root, dir.0);
    }

    #[test]
    fn folder_configs_stack_with_the_nearer_one_winning() {
        let dir = Dir::new("stack");
        dir.write(
            VAULT_CONFIG_FILE,
            "::config{ format: { table: { max_col_width: 50 }, link: { no_space: true } } }\n::macros{ m: \"vault\", v: \"vault\" }\n",
        );
        dir.write(
            "a/.config.tmt",
            "::config{ format: { table: { max_col_width: 60 } } }\n::macros{ m: \"outer\" }\n",
        );
        dir.write(
            "a/b/.config.tmt",
            "::config{ format: { table: { max_col_width: 70 } } }\n::macros{ m: \"inner\" }\n",
        );
        let deep = dir.write("a/b/note.tmt", "");
        let shallow = dir.write("a/note.tmt", "");

        let (deep_cfg, _, _) = find_config_file(&deep).unwrap();
        assert_eq!(deep_cfg.format.table_max_col_width, Some(70));
        assert!(
            deep_cfg.format.link_no_space,
            "the vault's sibling key survives"
        );
        assert_eq!(deep_cfg.macros["m"], "inner");
        assert_eq!(deep_cfg.macros["v"], "vault");

        let (shallow_cfg, _, _) = find_config_file(&shallow).unwrap();
        assert_eq!(shallow_cfg.format.table_max_col_width, Some(60));
        assert_eq!(shallow_cfg.macros["m"], "outer");
    }

    #[test]
    fn a_folder_config_beside_the_vault_config_stacks_on_it() {
        let dir = Dir::new("rootfolder");
        dir.write(
            VAULT_CONFIG_FILE,
            "::config{ format: { table: { max_col_width: 50 } } }\n",
        );
        dir.write(
            FOLDER_CONFIG_FILE,
            "::config{ format: { table: { max_col_width: 90 } } }\n",
        );
        let note = dir.write("note.tmt", "");
        let (cfg, _, _) = find_config_file(&note).unwrap();
        assert_eq!(cfg.format.table_max_col_width, Some(90));
    }

    #[test]
    fn the_nearest_vault_config_ends_the_walk() {
        let dir = Dir::new("nested");
        dir.write(
            VAULT_CONFIG_FILE,
            "::config{ format: { link: { no_space: true } } }\n",
        );
        dir.write("inner/vault.config.tmt", "::kind(config)\n");
        let note = dir.write("inner/note.tmt", "");
        let (cfg, file, root) = find_config_file(&note).unwrap();
        assert_eq!(root, dir.0.join("inner"));
        assert_eq!(file, dir.0.join("inner").join(VAULT_CONFIG_FILE));
        assert!(
            !cfg.format.link_no_space,
            "the outer vault does not leak in"
        );
    }

    #[test]
    fn a_folder_config_cannot_set_what_only_the_vault_sets() {
        let dir = Dir::new("vaultonly");
        dir.write(
            VAULT_CONFIG_FILE,
            "::config{ workspace: { ignore: list(\"target\") } }\n",
        );
        dir.write(
            "a/.config.tmt",
            "::config{ workspace: { ignore: list(\"elsewhere\") }, format: { link: { no_space: true } } }\n",
        );
        let note = dir.write("a/note.tmt", "");
        let (cfg, _, _) = find_config_file(&note).unwrap();
        assert_eq!(cfg.workspace.ignore, vec!["target".to_string()]);
        assert!(
            cfg.format.link_no_space,
            "the rest of that folder config still applies"
        );
    }

    #[test]
    fn the_document_config_is_the_top_layer() {
        let dir = Dir::new("document");
        dir.write(
            VAULT_CONFIG_FILE,
            "::config{ format: { table: { max_col_width: 50 }, link: { no_space: true } } }\n",
        );
        let note = dir.write("note.tmt", "");
        let src = "::config{ format: { table: { max_col_width: 99 } } }\n";
        let cfg = config_for(Some(&note), src);
        assert_eq!(cfg.format.table_max_col_width, Some(99));
        assert!(cfg.format.link_no_space);
    }

    #[test]
    fn without_a_vault_the_document_config_alone_applies() {
        let dir = Dir::new("lone");
        let note = dir.write("note.tmt", "");
        let cfg = config_for(
            Some(&note),
            "::config{ format: { link: { no_space: true } } }\n",
        );
        assert!(cfg.format.link_no_space);
    }
}
