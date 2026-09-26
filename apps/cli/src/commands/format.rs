use std::fs;
use std::path::{Path, PathBuf};

/// Formats one file's source through the config that governs it.
///
/// `tomet format` used to call `format_source`, which takes no config at
/// all, while the LSP formatted with the vault's. So a document the
/// editor wrote on save was a document `format --check` called
/// unformatted -- a guard rejecting what the tool everyone uses
/// produces. Both go through `tomet_config::config_for` now.
fn format_file(path: &Path, src: &str) -> String {
    let config = tomet_config::config_for(Some(path), src);
    tomet_formatter::format_source_with_config(src, &config)
}

/// Whether the config governing `path` leaves it alone
/// (`workspace.ignore` / `workspace.unswept`).
///
/// A directory sweep already skips such files, but a path named on the
/// command line used to be taken as-is. That made `tomet format -i` over an
/// explicit file list -- `git ls-files`, a shell glob -- rewrite the frozen
/// corpus and its references, which exist to be left exactly as they are.
fn is_excluded_by_config(path: &Path) -> bool {
    tomet_config::find_config_file(path)
        .is_some_and(|(config, _, root)| tomet_indexer::is_excluded_by_config(path, &root, &config))
}

/// Every `.tmt` file under `dir`; with `force`, the config's exclusions
/// (`ignore`, `unswept`) are not applied. `.gitignore` still is.
fn collect_dir(dir: &Path, force: bool) -> Vec<PathBuf> {
    if !force {
        return tomet_indexer::collect_tm_files(dir);
    }
    let (mut config, _, root) = tomet_config::find_config_file(dir).unwrap_or_else(|| {
        (
            tomet_config::PrinterConfig::default(),
            dir.to_path_buf(),
            dir.to_path_buf(),
        )
    });
    config.ignore_files.clear();
    config.unswept_files.clear();
    tomet_indexer::collect_tm_files_with_config(dir, &config, &root)
}

pub(crate) fn format_cmd(
    paths: &[PathBuf],
    write: bool,
    check: bool,
    force: bool,
) -> anyhow::Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    if paths.len() == 1 && paths[0].is_file() && !write && !check {
        let src = fs::read_to_string(&paths[0])?;
        let formatted = format_file(&paths[0], &src);
        print!("{formatted}");
        return Ok(());
    }

    let mut files = Vec::new();
    let mut skipped = 0usize;
    for path in paths {
        if path.is_file() {
            if !force && is_excluded_by_config(path) {
                eprintln!(
                    "skipped {}: excluded by the project config (workspace.ignore / \
                     workspace.unswept); pass --force to include it",
                    path.display()
                );
                skipped += 1;
            } else {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            files.extend(collect_dir(path, force));
        } else {
            return Err(anyhow::anyhow!("path '{}' does not exist", path.display()));
        }
    }

    if files.is_empty() {
        if skipped == 0 {
            println!("No .tmt files found");
        }
        return Ok(());
    }

    if check {
        let mut unformatted = Vec::new();
        for file in &files {
            let src = fs::read_to_string(file)?;
            if format_file(file, &src) != src {
                unformatted.push(file);
            }
        }
        if unformatted.is_empty() {
            return Ok(());
        }
        for file in &unformatted {
            println!("{}", file.display());
        }
        return Err(anyhow::anyhow!(
            "{} of {} file(s) not formatted",
            unformatted.len(),
            files.len()
        ));
    }

    if write {
        let mut changed = 0;
        for file in &files {
            match fs::read_to_string(file) {
                Ok(src) => {
                    let formatted = format_file(file, &src);
                    if formatted == src {
                        continue;
                    }
                    match fs::write(file, formatted) {
                        Ok(()) => {
                            if files.len() > 1 || paths[0].is_dir() {
                                println!("{}", file.display());
                            }
                            changed += 1;
                        }
                        Err(e) => eprintln!("Error writing {}: {e}", file.display()),
                    }
                }
                Err(e) => eprintln!("Error reading {}: {e}", file.display()),
            }
        }
        if files.len() > 1 || paths[0].is_dir() {
            println!("Formatted {changed} of {} file(s).", files.len());
        }
        return Ok(());
    }

    for file in &files {
        let src = fs::read_to_string(file)?;
        let formatted = format_file(file, &src);
        println!("==> {} <==", file.display());
        print!("{formatted}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// The formatter must use the config that governs the file, not the
    /// built-in defaults.
    ///
    /// This is the bug the helper above exists for: the LSP formatted
    /// with the vault's config and `tomet format` did not, so a document
    /// the editor wrote on save was one `format --check` called
    /// unformatted. `docs/README.tmt` was living proof -- the committed
    /// copy failed the config-aware check and the editor's copy passed.
    #[test]
    fn formatting_follows_the_vault_config_and_not_the_defaults() {
        let vault =
            std::env::temp_dir().join(format!("tomet_test_fmt_config_{}", nanoid::nanoid!()));
        let _ = fs::create_dir_all(&vault);
        fs::write(
            vault.join("default.config.tmt"),
            "@kind(config)\n@config(format:json)+++\n{ \"format\": { \"table\": { \"adjust_width\": \"auto\" } } }\n+++\n",
        )
        .unwrap();

        let doc = vault.join("t.tmt");
        let ragged = "#[ T ]\n\n@table[\n[ a ][ bbbbbb ]\n[ cccccc ][ d ]\n]\n";
        fs::write(&doc, ragged).unwrap();

        let with_config = format_file(&doc, ragged);
        assert_ne!(
            with_config,
            tomet_formatter::format_source(ragged),
            "the vault config has to change the answer, or this test proves nothing"
        );

        // And what the config-aware pass produces is what --check accepts.
        fs::write(&doc, &with_config).unwrap();
        assert!(format_cmd(std::slice::from_ref(&doc), false, true, false).is_ok());

        fs::write(&doc, ragged).unwrap();
        assert!(
            format_cmd(&[doc], false, true, false).is_err(),
            "--check must reject a file the config-aware formatter would rewrite"
        );

        let _ = fs::remove_dir_all(&vault);
    }

    #[test]
    fn test_format_multiple_files_in_place() {
        let temp_dir =
            std::env::temp_dir().join(format!("tomet_test_fmt_multi_{}", nanoid::nanoid!()));
        let _ = fs::create_dir_all(&temp_dir);
        let file1 = temp_dir.join("a.tmt");
        let file2 = temp_dir.join("b.tmt");

        fs::write(&file1, "# Heading   \n\n\nBody   \n").unwrap();
        fs::write(&file2, "# Heading 2   \n").unwrap();

        let paths = vec![file1.clone(), file2.clone()];
        format_cmd(&paths, true, false, false).unwrap();

        let res1 = fs::read_to_string(&file1).unwrap();
        let res2 = fs::read_to_string(&file2).unwrap();

        assert_eq!(res1, "# Heading\n\nBody\n");
        assert_eq!(res2, "# Heading 2\n");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_format_check_mode() {
        let temp_dir =
            std::env::temp_dir().join(format!("tomet_test_fmt_check_{}", nanoid::nanoid!()));
        let _ = fs::create_dir_all(&temp_dir);
        let file = temp_dir.join("c.tmt");

        fs::write(&file, "# Heading   \n").unwrap();
        let paths = vec![file.clone()];
        assert!(format_cmd(&paths, false, true, false).is_err());

        format_cmd(&paths, true, false, false).unwrap();
        assert!(format_cmd(&paths, false, true, false).is_ok());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    /// A file named on the command line is held to the same exclusions as a
    /// directory sweep, and `--force` lifts them.
    #[test]
    fn an_explicit_file_the_config_leaves_alone_is_skipped_until_forced() {
        let vault =
            std::env::temp_dir().join(format!("tomet_test_fmt_unswept_{}", nanoid::nanoid!()));
        let frozen_dir = vault.join("tests").join("fixtures");
        let _ = fs::create_dir_all(&frozen_dir);
        fs::write(
            vault.join("default.config.tmt"),
            "@kind(config)\n@config(format:json)+++\n{ \"workspace\": { \"unswept\": [\"tests/fixtures\"] } }\n+++\n",
        )
        .unwrap();
        let frozen = frozen_dir.join("f.tmt");
        let normal = vault.join("n.tmt");
        let ragged = "# Heading   \n";
        fs::write(&frozen, ragged).unwrap();
        fs::write(&normal, ragged).unwrap();

        // Both named explicitly: only the ordinary one is rewritten.
        format_cmd(&[frozen.clone(), normal.clone()], true, false, false).unwrap();
        assert_eq!(fs::read_to_string(&frozen).unwrap(), ragged);
        assert_eq!(fs::read_to_string(&normal).unwrap(), "# Heading\n");

        // `--check` does not count a skipped file against the run.
        assert!(format_cmd(std::slice::from_ref(&frozen), false, true, false).is_ok());

        // A directory sweep skips it too, and `--force` reaches it either way.
        format_cmd(std::slice::from_ref(&vault), true, false, false).unwrap();
        assert_eq!(fs::read_to_string(&frozen).unwrap(), ragged);
        format_cmd(std::slice::from_ref(&frozen), true, false, true).unwrap();
        assert_eq!(fs::read_to_string(&frozen).unwrap(), "# Heading\n");

        fs::write(&frozen, ragged).unwrap();
        format_cmd(std::slice::from_ref(&vault), true, false, true).unwrap();
        assert_eq!(fs::read_to_string(&frozen).unwrap(), "# Heading\n");

        let _ = fs::remove_dir_all(&vault);
    }
}
