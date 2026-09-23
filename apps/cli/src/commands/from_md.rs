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
    let (config, _, _) = tomet_config::find_config_file(path).unwrap_or((
        tomet_config::PrinterConfig::default(),
        path.to_path_buf(),
        path.to_path_buf(),
    ));
    let import_opts = tomet_markdown::ImportOptions {
        adjust_table_width: config.table_adjust_width.as_deref() == Some("true")
            || config.table_adjust_width.as_deref() == Some("auto"),
        table_adjust_width_mode: config.table_adjust_width.clone(),
        table_max_col_width: config.table_max_col_width,
        table_align: config.table_align.clone(),
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
    let (config, _, config_root) = tomet_config::find_config_file(dir_path).unwrap_or((
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
                succeeded += 1;
                if !dry_run {
                    if let Some(out_dir) = out {
                        let rel = candidate
                            .source_path
                            .strip_prefix(dir_path)
                            .unwrap_or(&candidate.source_path);
                        let dest = out_dir.join(rel).with_extension("tmt");
                        if let Some(parent) = dest.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        if let Err(e) = fs::write(&dest, &tmt) {
                            eprintln!("Error writing {}: {e}", dest.display());
                        } else if remove_original && candidate.source_path != dest {
                            let _ = fs::remove_file(&candidate.source_path);
                        }
                    } else if in_place {
                        let dest = &candidate.target_path;
                        if let Err(e) = fs::write(dest, &tmt) {
                            eprintln!("Error writing {}: {e}", dest.display());
                        } else if remove_original && candidate.source_path != *dest {
                            let _ = fs::remove_file(&candidate.source_path);
                        }
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
}
