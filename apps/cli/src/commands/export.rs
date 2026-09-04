use std::fs;
use std::path::{Path, PathBuf};

use tomet_semantics::{ExportType, document_config};

use crate::util::{format_parse_error, meta_title};

pub(crate) fn export_cmd(
    target_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
    check: bool,
) -> anyhow::Result<()> {
    if target_path.is_file() {
        export_single_file(target_path, override_type, override_out, advanced, check)
    } else if target_path.is_dir() {
        export_directory(target_path, override_type, override_out, advanced, check)
    } else {
        Err(anyhow::anyhow!(
            "path '{}' does not exist",
            target_path.display()
        ))
    }
}

/// Set by `--check`: a generated file that disagrees with its source is a
/// failure with a mechanical fix, which beats a rule asking people to
/// remember not to hand-edit it.
#[derive(Debug, Default)]
pub(crate) struct StaleExports(pub Vec<PathBuf>);

fn export_single_file(
    file_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
    check: bool,
) -> anyhow::Result<()> {
    let mut stale = StaleExports::default();
    export_one(
        file_path,
        override_type,
        override_out,
        advanced,
        check,
        &mut stale,
    )?;
    report_stale(&stale, check)
}

/// Non-zero exit when `--check` found something out of date, so this can
/// gate a commit the way `format --check` and `refactor --check` do.
pub(crate) fn report_stale(stale: &StaleExports, check: bool) -> anyhow::Result<()> {
    if !check || stale.0.is_empty() {
        if check {
            println!("All exports are up to date.");
        }
        return Ok(());
    }
    for path in &stale.0 {
        println!("Stale export: {}", path.display());
    }
    Err(anyhow::anyhow!(
        "{} export(s) differ from their source; re-run without --check",
        stale.0.len()
    ))
}

fn export_one(
    file_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
    check: bool,
    stale: &mut StaleExports,
) -> anyhow::Result<()> {
    let src = fs::read_to_string(file_path)?;
    let doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file_path, &src, &e)))?;

    let config = document_config(&doc);

    // Determine target formats to export
    let targets = if let Some(t_str) = override_type {
        if t_str.eq_ignore_ascii_case("all") {
            if !config.export_type.is_empty() {
                config.export_type.clone()
            } else {
                vec![ExportType::CommonMark, ExportType::Html]
            }
        } else {
            vec![ExportType::parse(t_str)]
        }
    } else if !config.export_type.is_empty() {
        config.export_type.clone()
    } else {
        vec![ExportType::CommonMark]
    };

    for target in &targets {
        let ext = match target {
            ExportType::CommonMark => "md",
            ExportType::Html => "html",
            ExportType::Typst => "typ",
            ExportType::Pandoc => "json",
            ExportType::Custom(s) => s.as_str(),
        };

        let out_path = if let Some(out) = override_out {
            if out.is_dir() || (override_out.is_some() && (targets.len() > 1 || file_path.is_dir()))
            {
                let stem = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                Some(out.join(format!("{stem}.{ext}")))
            } else {
                Some(out.to_path_buf())
            }
        } else if let Some(cfg_path) = config.export_path_for(target) {
            Some(PathBuf::from(cfg_path))
        } else if targets.len() == 1 {
            None
        } else {
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            Some(PathBuf::from(format!("{stem}.{ext}")))
        };

        let rendered = match target {
            ExportType::CommonMark => tomet_markdown::to_markdown(&doc),
            // Pandoc's AST, not a rendering -- `pandoc -f json` turns it
            // into whatever format is actually wanted.
            ExportType::Pandoc => serde_json::to_string(&tomet_pandoc::to_pandoc(&doc))?,
            ExportType::Html => {
                let filename_title = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Tomet");
                let title = meta_title(&doc).unwrap_or_else(|| filename_title.to_string());
                let options = tomet_html::RenderOptions {
                    number_headings: advanced,
                    auto_slug_headings: advanced,
                    lang: None,
                };
                tomet_html::render_page_with(&doc, &title, &options)
            }
            ExportType::Typst => tomet_typst::to_typst(&doc),
            ExportType::Custom(fmt) => {
                return Err(anyhow::anyhow!("unsupported export type: {fmt}"));
            }
        };

        if let Some(dest) = out_path {
            if check {
                // Missing counts as stale: the source says a file should be
                // there, so its absence is exactly what this is for.
                let current = fs::read_to_string(&dest).unwrap_or_default();
                if current != rendered {
                    stale.0.push(dest);
                }
                continue;
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, &rendered)?;
            println!("Exported {} -> {}", file_path.display(), dest.display());
        } else if !check {
            print!("{rendered}");
        }
    }

    Ok(())
}

fn export_directory(
    dir_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
    check: bool,
) -> anyhow::Result<()> {
    let files = tomet_indexer::collect_tm_files(dir_path);
    if files.is_empty() {
        println!("No .tmt or .tmt files found in {}", dir_path.display());
        return Ok(());
    }

    let mut exported_count = 0;
    let mut stale = StaleExports::default();
    for file in &files {
        let relative_out = if let Some(out_dir) = override_out {
            if let Ok(rel) = file.strip_prefix(dir_path) {
                let parent = rel.parent().unwrap_or_else(|| Path::new(""));
                Some(out_dir.join(parent))
            } else {
                Some(out_dir.to_path_buf())
            }
        } else {
            None
        };

        match export_one(
            file,
            override_type,
            relative_out.as_deref(),
            advanced,
            check,
            &mut stale,
        ) {
            Ok(()) => exported_count += 1,
            Err(e) => eprintln!("Error exporting {}: {e}", file.display()),
        }
    }

    if check {
        return report_stale(&stale, check);
    }
    println!("Batch export finished: {exported_count} file(s) processed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--check` has to fail before it is worth running. Exports once,
    /// confirms clean, edits the output by hand, confirms it is caught.
    #[test]
    fn check_mode_catches_a_hand_edited_export() {
        let dir = std::env::temp_dir().join("tomet_test_export_check");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let src = dir.join("doc.tmt");
        let out = dir.join("doc.md");
        fs::write(
            &src,
            format!(
                "@config(export: {{ type: commonmark, path: \"{}\" }})\n\n#[ Title ]\n",
                out.display()
            ),
        )
        .expect("write source");

        export_cmd(&src, None, None, false, false).expect("export writes");
        export_cmd(&src, None, None, false, true).expect("freshly exported output is up to date");

        fs::write(&out, "# Hand edited\n").expect("tamper");
        assert!(
            export_cmd(&src, None, None, false, true).is_err(),
            "a hand-edited export must not pass --check"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_single_file_with_config_writes_output_file() {
        let temp_dir = std::env::temp_dir().join("tomet_test_export");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("test_doc.tmt");
        let out_file = temp_dir.join("test_out.md");

        let src_content = format!(
            "@config(\n  export: {{\n    type: commonmark\n    path: \"{}\"\n  }}\n)\n#[ Hello Export ]\n",
            out_file.display()
        );
        fs::write(&src_file, src_content).unwrap();

        let res = export_cmd(&src_file, None, None, false, false);
        assert!(res.is_ok());
        assert!(out_file.exists());
        let exported_text = fs::read_to_string(&out_file).unwrap();
        assert!(exported_text.contains("# Hello Export"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
