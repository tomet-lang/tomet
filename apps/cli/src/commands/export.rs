use std::fs;
use std::path::{Path, PathBuf};

use tomet_semantics::{ExportType, document_config};

use crate::util::{format_parse_error, meta_title};

pub(crate) fn export_cmd(
    target_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
) -> anyhow::Result<()> {
    if target_path.is_file() {
        export_single_file(target_path, override_type, override_out, advanced)
    } else if target_path.is_dir() {
        export_directory(target_path, override_type, override_out, advanced)
    } else {
        Err(anyhow::anyhow!(
            "path '{}' does not exist",
            target_path.display()
        ))
    }
}

fn export_single_file(
    file_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
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
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, &rendered)?;
            println!("Exported {} -> {}", file_path.display(), dest.display());
        } else {
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
) -> anyhow::Result<()> {
    let files = tomet_indexer::collect_tm_files(dir_path);
    if files.is_empty() {
        println!("No .tmt or .tmt files found in {}", dir_path.display());
        return Ok(());
    }

    let mut exported_count = 0;
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

        match export_single_file(file, override_type, relative_out.as_deref(), advanced) {
            Ok(()) => exported_count += 1,
            Err(e) => eprintln!("Error exporting {}: {e}", file.display()),
        }
    }

    println!("Batch export finished: {exported_count} file(s) processed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let res = export_cmd(&src_file, None, None, false);
        assert!(res.is_ok());
        assert!(out_file.exists());
        let exported_text = fs::read_to_string(&out_file).unwrap();
        assert!(exported_text.contains("# Hello Export"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
