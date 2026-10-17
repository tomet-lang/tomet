use std::fs;
use std::path::Path;

use crate::util::format_parse_error;

pub(crate) fn from_md_cmd(
    path: &Path,
    out: Option<&Path>,
    in_place: bool,
    remove_original: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    if path.is_file() {
        from_md_single_file(path, out, in_place, remove_original, dry_run)
    } else if path.is_dir() {
        from_md_directory(path, out, in_place, remove_original, dry_run)
    } else {
        Err(anyhow::anyhow!("path '{}' does not exist", path.display()))
    }
}

fn convert_md_source(path: &Path, src: &str) -> anyhow::Result<String> {
    let (config, _, _) = tomet_load_config::find_config_file(path).unwrap_or((
        tomet_config::PrinterConfig::default(),
        path.to_path_buf(),
        path.to_path_buf(),
    ));
    let import_opts = tomet_markdown::ImportOptions {
        adjust_table_width: config.format.table_adjust_width.as_deref() == Some("true")
            || config.format.table_adjust_width.as_deref() == Some("auto"),
        table_adjust_width_mode: config.format.table_adjust_width.clone(),
        table_max_col_width: config.format.table_max_col_width,
        table_align: config.format.table_align.clone(),
    };
    let mut doc = tomet_markdown::from_markdown_with_options(src, &import_opts);
    tomet_printer::ensure_document_id_with_config(&mut doc, &config);
    let tmt = tomet_printer::document_to_tm_with_config(&doc, &config);
    if let Err(e) = tomet_parser::parse_document(&tmt) {
        return Err(anyhow::anyhow!(
            "generated .tmt has parse error: {}",
            format_parse_error(path, &tmt, &e)
        ));
    }
    Ok(tmt)
}

fn from_md_single_file(
    file_path: &Path,
    out: Option<&Path>,
    in_place: bool,
    remove_original: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let src = fs::read_to_string(file_path)?;
    let tmt = convert_md_source(file_path, &src)?;

    if dry_run {
        println!("OK (dry-run): {}", file_path.display());
        return Ok(());
    }

    if let Some(out_path) = out {
        let dest = if out_path.is_dir() {
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            out_path.join(format!("{stem}.tmt"))
        } else {
            out_path.to_path_buf()
        };
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &tmt)?;
        if remove_original && file_path != dest {
            let _ = fs::remove_file(file_path);
        }
        println!("Converted {} -> {}", file_path.display(), dest.display());
    } else if in_place {
        let dest = file_path.with_extension("tmt");
        fs::write(&dest, &tmt)?;
        if remove_original && file_path != dest {
            let _ = fs::remove_file(file_path);
        }
        println!("Converted {} -> {}", file_path.display(), dest.display());
    } else {
        print!("{tmt}");
    }
    Ok(())
}

fn from_md_directory(
    dir_path: &Path,
    out: Option<&Path>,
    in_place: bool,
    remove_original: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    // A directory has no standard output to go to. With nowhere named to write,
    // everything would be converted and nothing kept, and the summary would
    // read as if it had been.
    if !dry_run && out.is_none() && !in_place {
        return Err(anyhow::anyhow!(
            "a directory is converted into files: pass --out <DIR> or --in-place (or --dry-run to only check)"
        ));
    }
    let (config, _, config_root) = tomet_load_config::find_config_file(dir_path).unwrap_or((
        tomet_config::PrinterConfig::default(),
        dir_path.to_path_buf(),
        dir_path.to_path_buf(),
    ));

    let index =
        tomet_indexer::workspace_scan::WorkspaceIndex::build(dir_path, &config, &config_root);
    let candidates = index.migration_candidates();

    if candidates.is_empty() {
        println!("No .md files found in {}", dir_path.display());
        return Ok(());
    }

    let mut succeeded = 0;
    let mut failed = 0;
    let mut error_reports = Vec::new();

    for candidate in &candidates {
        let src = match fs::read_to_string(&candidate.source_path) {
            Ok(s) => s,
            Err(e) => {
                failed += 1;
                error_reports.push(format!(
                    "{}: read error: {e}",
                    candidate.source_path.display()
                ));
                continue;
            }
        };

        match convert_md_source(&candidate.source_path, &src) {
            Ok(tmt) => {
                // Converted is not written: a file only counts once it is
                // where it was asked to go.
                let written = if dry_run {
                    Ok(candidate.target_path.clone())
                } else if let Some(out_dir) = out {
                    let rel = candidate
                        .source_path
                        .strip_prefix(dir_path)
                        .unwrap_or(&candidate.source_path);
                    write_converted(&out_dir.join(rel).with_extension("tmt"), &tmt)
                } else {
                    write_converted(&candidate.target_path, &tmt)
                };
                match written {
                    Ok(dest) => {
                        succeeded += 1;
                        if remove_original && !dry_run && candidate.source_path != dest {
                            let _ = fs::remove_file(&candidate.source_path);
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        error_reports.push(format!("{}: {e}", candidate.source_path.display()));
                    }
                }
            }
            Err(e) => {
                failed += 1;
                error_reports.push(format!("{}: {e}", candidate.source_path.display()));
            }
        }
    }

    if dry_run {
        println!(
            "Dry-run finished: {} total, {} succeeded, {} failed",
            candidates.len(),
            succeeded,
            failed
        );
    } else {
        println!(
            "Conversion finished: {} total, {} succeeded, {} failed",
            candidates.len(),
            succeeded,
            failed
        );
    }

    if !error_reports.is_empty() {
        eprintln!("\nErrors encountered ({}):", error_reports.len());
        for err in &error_reports {
            eprintln!("  {err}");
        }
    }

    if failed > 0 {
        Err(anyhow::anyhow!("{failed} file(s) failed conversion"))
    } else {
        Ok(())
    }
}

/// Writes `tmt` to `dest`, making its directory if it has none, and gives back
/// where it went.
fn write_converted(dest: &Path, tmt: &str) -> Result<std::path::PathBuf, String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    fs::write(dest, tmt).map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
    Ok(dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_md_single_file_converts_markdown() {
        let temp_dir = std::env::temp_dir().join("tomet_test_from_md");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("test_doc.md");
        let out_file = temp_dir.join("test_doc.tmt");

        let md_content =
            "---\naliases:\n  - Hello\n---\n\n# Title\n\n- [[Other Note]]\n- bare text\n";
        fs::write(&src_file, md_content).unwrap();

        let res = from_md_cmd(&src_file, Some(&out_file), false, false, false);
        assert!(res.is_ok());
        assert!(out_file.exists());
        let tmt_text = fs::read_to_string(&out_file).unwrap();
        assert!(tmt_text.contains("=[Title]"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    fn dir_with_markdown(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tomet_test_from_md_dir_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.md"), "# A\n\ntext\n").unwrap();
        fs::write(dir.join("b.md"), "# B\n\ntext\n").unwrap();
        dir
    }

    /// A directory has no standard output to go to, so with nowhere named to
    /// write it used to convert everything, write nothing, and say
    /// "Conversion finished: 2 succeeded" -- which reads as having converted.
    #[test]
    fn a_directory_with_nowhere_to_write_is_an_error_and_writes_nothing() {
        let dir = dir_with_markdown("nowhere");
        let result = from_md_cmd(&dir, None, false, false, false);
        assert!(result.is_err(), "nothing was asked to be written");
        assert!(!dir.join("a.tmt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dry_run_of_a_directory_needs_no_destination() {
        let dir = dir_with_markdown("dry");
        from_md_cmd(&dir, None, false, false, true).expect("a dry run only checks");
        assert!(!dir.join("a.tmt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A file that was converted but could not be written was not converted.
    /// It used to stay counted as a success, with the exit code 0.
    #[test]
    fn a_file_that_could_not_be_written_is_a_failure() {
        let dir = dir_with_markdown("write");
        // `a.tmt` is a directory, so writing the file there fails.
        fs::create_dir_all(dir.join("a.tmt")).unwrap();
        let result = from_md_cmd(&dir, None, true, false, false);
        assert!(result.is_err(), "a failed write must fail the run");
        assert!(dir.join("b.tmt").is_file(), "the others are still done");
        // The original of what was not written is not removed.
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_does_not_remove_the_original() {
        let dir = dir_with_markdown("keep");
        fs::create_dir_all(dir.join("a.tmt")).unwrap();
        let _ = from_md_cmd(&dir, None, true, true, false);
        assert!(dir.join("a.md").is_file(), "not converted, so not removed");
        assert!(!dir.join("b.md").exists(), "converted, so removed");
        let _ = fs::remove_dir_all(&dir);
    }
}
