use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rayon::prelude::*;

enum FormatWriteResult {
    Unchanged,
    Written,
    ReadError(io::Error),
    WriteError(io::Error),
}

/// Formats one file's source through the config that governs it.
///
/// `tomet format` used to call `format_source`, which takes no config at
/// all, while the LSP formatted with the vault's. So a document the
/// editor wrote on save was a document `format --check` called
/// unformatted -- a guard rejecting what the tool everyone uses
/// produces. Both go through `tomet_load_config::config_for` now.
///
/// A `.ctmt` is formatted by whitespace alone, whatever the config says: it is
/// data with no tables or element groups, and reprinting it from its value
/// would drop the comments.
fn format_file(path: &Path, src: &str) -> String {
    if tomet_indexer::is_ctmt_file(path) {
        return tomet_formatter::format_data_source(src);
    }
    let config = tomet_load_config::config_for(Some(path), src);
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
    tomet_load_config::find_config_file(path)
        .is_some_and(|(config, _, root)| tomet_indexer::is_excluded_by_config(path, &root, &config))
}

/// Every `.tmt` and `.ctmt` file under `dir`; with `force`, the config's exclusions
/// (`ignore`, `unswept`) are not applied. `.gitignore` still is.
fn collect_dir(dir: &Path, force: bool) -> Vec<PathBuf> {
    if !force {
        return tomet_indexer::collect_tm_and_ctmt_files(dir);
    }
    let (mut config, _, root) = tomet_load_config::find_config_file(dir).unwrap_or_else(|| {
        (
            tomet_config::PrinterConfig::default(),
            dir.to_path_buf(),
            dir.to_path_buf(),
        )
    });
    config.workspace.ignore.clear();
    config.workspace.unswept.clear();
    tomet_indexer::collect_tm_and_ctmt_files_with_config(dir, &config, &root)
}

/// Formats one or more files or directories.
///
/// Under `--check`, formatting checks run in parallel across rayon worker threads
/// governed by the jobserver gate, reporting unformatted files in file order, or
/// aborting on the first unreadable file in file order.
///
/// Under `-i` (write), files are formatted and written in parallel across worker
/// threads governed by the jobserver gate. Before this change, a failed write to
/// standard output (for example piped to `head`) stopped the loop at that file and
/// left subsequent files unformatted. Now every file is written first, and the
/// file listing is printed afterwards.
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
        let mut out = io::stdout().lock();
        out.write_all(formatted.as_bytes())?;
        return Ok(());
    }

    let mut files = Vec::with_capacity(paths.len());
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
            println!("No .tmt or .ctmt files found");
        }
        return Ok(());
    }

    if check {
        // Phase 1: Per-file, in parallel, pure
        let results: Vec<io::Result<bool>> = files
            .par_iter()
            .map(|file| {
                let _token = jobserver_gate::acquire();
                let src = fs::read_to_string(file)?;
                let is_unformatted = format_file(file, &src) != src;
                Ok(is_unformatted)
            })
            .collect();
        jobserver_gate::reclaim_implicit_token();

        // Phase 2: In order, serial
        let mut unformatted = Vec::new();
        for (file, res) in files.iter().zip(results) {
            let is_unformatted = res?;
            if is_unformatted {
                unformatted.push(file);
            }
        }
        if unformatted.is_empty() {
            return Ok(());
        }
        let mut out = io::stdout().lock();
        for file in &unformatted {
            writeln!(out, "{}", file.display())?;
        }
        return Err(anyhow::anyhow!(
            "{} of {} file(s) not formatted",
            unformatted.len(),
            files.len()
        ));
    }

    if write {
        // Phase 1: Per-file, in parallel, writes directly
        let results: Vec<FormatWriteResult> = files
            .par_iter()
            .map(|file| {
                let _token = jobserver_gate::acquire();
                match fs::read_to_string(file) {
                    Ok(src) => {
                        let formatted = format_file(file, &src);
                        if formatted == src {
                            FormatWriteResult::Unchanged
                        } else {
                            match fs::write(file, formatted) {
                                Ok(()) => FormatWriteResult::Written,
                                Err(e) => FormatWriteResult::WriteError(e),
                            }
                        }
                    }
                    Err(e) => FormatWriteResult::ReadError(e),
                }
            })
            .collect();
        jobserver_gate::reclaim_implicit_token();

        // Phase 2: In order, serial
        let mut changed = 0;
        let mut out = io::stdout().lock();
        for (file, res) in files.iter().zip(results) {
            match res {
                FormatWriteResult::Unchanged => {}
                FormatWriteResult::Written => {
                    if files.len() > 1 || paths[0].is_dir() {
                        writeln!(out, "{}", file.display())?;
                    }
                    changed += 1;
                }
                FormatWriteResult::WriteError(e) => {
                    eprintln!("Error writing {}: {e}", file.display());
                }
                FormatWriteResult::ReadError(e) => {
                    eprintln!("Error reading {}: {e}", file.display());
                }
            }
        }
        if files.len() > 1 || paths[0].is_dir() {
            writeln!(out, "Formatted {changed} of {} file(s).", files.len())?;
        }
        return Ok(());
    }

    let mut out = io::stdout().lock();
    for file in &files {
        let src = fs::read_to_string(file)?;
        let formatted = format_file(file, &src);
        writeln!(out, "==> {} <==", file.display())?;
        out.write_all(formatted.as_bytes())?;
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
            vault.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::config{ format: { table: { adjust_width: auto } } }\n",
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

    /// A `.ctmt` is formatted by whitespace only, in a directory sweep as well
    /// as when named, and a comment survives.
    #[test]
    fn a_cotomet_file_is_formatted_beside_the_documents() {
        let dir = std::env::temp_dir().join(format!("tomet_test_fmt_ctmt_{}", nanoid::nanoid!()));
        let _ = fs::create_dir_all(&dir);
        let doc = dir.join("a.tmt");
        let data = dir.join("catasto.ctmt");
        fs::write(&doc, "# Heading   \n").unwrap();
        fs::write(
            &data,
            "// keep me  \r\na: 1   \r\n\r\n\r\nb: \"x  \r\ny\"\r\n",
        )
        .unwrap();

        assert!(format_cmd(std::slice::from_ref(&data), false, true, false).is_err());
        format_cmd(std::slice::from_ref(&dir), true, false, false).unwrap();
        assert_eq!(fs::read_to_string(&doc).unwrap(), "# Heading\n");
        assert_eq!(
            fs::read_to_string(&data).unwrap(),
            "// keep me\na: 1\n\nb: \"x  \r\ny\"\n"
        );
        assert!(format_cmd(&[dir.clone()], false, true, false).is_ok());

        // one that does not parse is left as it is
        fs::write(&data, "a: \"open   \n\n\n").unwrap();
        format_cmd(&[data.clone()], true, false, false).unwrap();
        assert_eq!(fs::read_to_string(&data).unwrap(), "a: \"open   \n\n\n");

        let _ = fs::remove_dir_all(&dir);
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
            vault.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::config{ workspace: { unswept: list(\"tests/fixtures\") } }\n",
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
