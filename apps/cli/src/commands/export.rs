use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tomet_load::Vault;
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

struct OneFileExport {
    stdout_lines: Vec<String>,
    stdout_raw: Vec<String>,
    stale_paths: Vec<PathBuf>,
}

fn export_single_file(
    file_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
    check: bool,
) -> anyhow::Result<()> {
    let mut stale = StaleExports::default();
    let project_root = project_root_for(file_path);
    let vault = Vault::discover(file_path);
    let settings = ExportSettings {
        override_type,
        advanced,
        check,
        project_root: &project_root,
        vault: &vault,
    };
    match export_one(&settings, file_path, override_out) {
        Ok(out) => {
            for line in out.stdout_lines {
                println!("{line}");
            }
            for raw in out.stdout_raw {
                print!("{raw}");
            }
            stale.0.extend(out.stale_paths);
        }
        Err(e) => return Err(e),
    }
    report_stale(&stale, check)
}

/// Where a declared output path is measured from: the directory holding
/// the nearest `default.config.tmt`, which is what declares where the
/// vault begins.
///
/// Falls back to the target's own directory when there is no config to
/// find, which keeps a loose file exporting beside itself rather than
/// into whatever directory the shell happened to be in.
fn project_root_for(target: &Path) -> PathBuf {
    if let Some((_, _, root)) = tomet_load_config::find_config_file(target) {
        return root;
    }
    if target.is_dir() {
        return target.to_path_buf();
    }
    target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
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

/// What stays the same for every file of one export run.
#[derive(Clone, Copy)]
struct ExportSettings<'a> {
    override_type: Option<&'a str>,
    advanced: bool,
    check: bool,
    project_root: &'a Path,
    vault: &'a Vault,
}

fn export_one(
    settings: &ExportSettings,
    file_path: &Path,
    override_out: Option<&Path>,
) -> anyhow::Result<OneFileExport> {
    let ExportSettings {
        override_type,
        advanced,
        check,
        project_root,
        vault,
    } = *settings;

    let src = fs::read_to_string(file_path)?;
    // `vault.parse` and not `vault.document`, so the source text is still
    // in hand for `format_parse_error`'s pointer-at-the-column report.
    let (mut doc, _bindings) = vault
        .parse(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file_path, &src, &e)))?;

    // Every `${...}` is resolved before anything renders, so every converter
    // is handed its value and none of them needs to know expressions exist.
    vault.prepare(&mut doc, file_path);

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

    let mut out = OneFileExport {
        stdout_lines: Vec::new(),
        stdout_raw: Vec::new(),
        stale_paths: Vec::new(),
    };

    for target in &targets {
        let ext = match target {
            ExportType::CommonMark => "md",
            ExportType::Html => "html",
            ExportType::Typst => "typ",
            ExportType::Pandoc => "json",
            ExportType::Custom(s) => s.as_str(),
        };

        let out_path = if let Some(out_dir) = override_out {
            if out_dir.is_dir()
                || (override_out.is_some() && (targets.len() > 1 || file_path.is_dir()))
            {
                let stem = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                Some(out_dir.join(format!("{stem}.{ext}")))
            } else {
                Some(out_dir.to_path_buf())
            }
        } else if let Some(cfg_path) = config.export_path_for(target) {
            // A path a document declares means the same thing here as it
            // does in a link: bare is measured from the project root,
            // `./`/`../` from the document's own directory. This used to
            // hand the string straight to `fs::write`, which measured it
            // from the process's working directory instead -- so
            // `tmtroot/readme.tmt` declaring `README.ja.md` landed at the
            // repository root or under `docs/` depending on where you
            // stood, and `--check` only agreed with itself from one of
            // them.
            Some(tomet_indexer::resolve_document_relative(
                file_path,
                cfg_path,
                project_root,
            ))
        } else if targets.len() == 1 {
            None
        } else {
            let stem = file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            Some(PathBuf::from(format!("{stem}.{ext}")))
        };

        // Under `--check`, if there is no destination output path, there is no
        // artifact on disk to compare against. Skip rendering entirely.
        if check && out_path.is_none() {
            continue;
        }

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
                    ..Default::default()
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
                    out.stale_paths.push(dest);
                }
                continue;
            }
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&dest, &rendered)?;
            out.stdout_lines.push(format!(
                "Exported {} -> {}",
                file_path.display(),
                dest.display()
            ));
        } else if !check {
            out.stdout_raw.push(rendered);
        }
    }

    Ok(out)
}

/// Exports all `.tmt` files under a directory.
///
/// Running export across many documents parallelises per-file with rayon and
/// acquires tokens from the jobserver gate. Profiling shows that parsing and
/// rendering take ~0.65-1.2 ms per document (~35% source read, ~35% parse,
/// ~14% prepare, ~15% render), so multi-file sweeps scale near-linearly with
/// core count. When running with `--check`, documents declaring no export path
/// skip rendering entirely.
///
/// Diagnostics and output lines are emitted in file order after all files have
/// finished processing to ensure deterministic CLI output.
fn export_directory(
    dir_path: &Path,
    override_type: Option<&str>,
    override_out: Option<&Path>,
    advanced: bool,
    check: bool,
) -> anyhow::Result<()> {
    let files = tomet_indexer::collect_tm_files(dir_path);
    let project_root = project_root_for(dir_path);
    if files.is_empty() {
        println!("No .tmt or .tmt files found in {}", dir_path.display());
        return Ok(());
    }

    // Discovered once for the whole run; the metadata table inside it is
    // filled by whichever document asks first -- see `export_one`.
    let vault = Vault::discover(dir_path);
    let settings = ExportSettings {
        override_type,
        advanced,
        check,
        project_root: &project_root,
        vault: &vault,
    };

    // Phase 1: Process documents in parallel governed by jobserver tokens
    let results: Vec<Result<OneFileExport, String>> = files
        .par_iter()
        .map(|file| {
            let _token = jobserver_gate::acquire();
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
            export_one(&settings, file, relative_out.as_deref()).map_err(|e| e.to_string())
        })
        .collect();
    jobserver_gate::reclaim_implicit_token();

    // Phase 2: Emit output and diagnostics in exact file order
    let mut exported_count = 0;
    let mut failed = 0usize;
    let mut stale = StaleExports::default();

    for (file, res) in files.iter().zip(results) {
        match res {
            Ok(out) => {
                exported_count += 1;
                for line in out.stdout_lines {
                    println!("{line}");
                }
                for raw in out.stdout_raw {
                    print!("{raw}");
                }
                stale.0.extend(out.stale_paths);
            }
            Err(e) => {
                failed += 1;
                eprintln!("Error exporting {}: {e}", file.display());
            }
        }
    }

    // One document that fails does not stop the others, but it is not a
    // success: under `--check` a document that could not be read has not been
    // checked, and passing there says it is up to date.
    if check {
        if failed == 0 {
            return report_stale(&stale, check);
        }
        for path in &stale.0 {
            println!("Stale export: {}", path.display());
        }
        let differ = if stale.0.is_empty() {
            String::new()
        } else {
            format!(", and {} export(s) differ from their source", stale.0.len())
        };
        anyhow::bail!(
            "{failed} of {} file(s) could not be checked{differ}",
            files.len()
        );
    }

    if failed > 0 {
        println!("Batch export finished: {exported_count} file(s) processed.");
        anyhow::bail!("{failed} of {} file(s) could not be exported", files.len());
    }

    println!("Batch export finished: {exported_count} file(s) processed.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare declared path is measured from the project root -- the
    /// directory holding `default.config.tmt` -- and not from wherever
    /// the process happens to be standing.
    ///
    /// The source sits in a subdirectory and declares plain `out.md`.
    /// Landing beside the source, or in the test runner's own working
    /// directory, are the two ways this used to go wrong; both are
    /// asserted against. `tmtroot/` depends on this: its whole point is
    /// that a source in one place produces a file in another.
    #[test]
    fn a_bare_export_path_is_measured_from_the_project_root() {
        let root = std::env::temp_dir().join("tomet_test_export_root");
        let _ = fs::remove_dir_all(&root);
        let sub = root.join("sub");
        fs::create_dir_all(&sub).expect("temp dirs");
        fs::write(root.join("default.config.tmt"), "::kind(config)\n").expect("config");

        let src = sub.join("doc.tmt");
        fs::write(
            &src,
            "::config(export: { type: commonmark, path: \"out.md\" })\n\n#[ Title ]\n",
        )
        .expect("write source");

        export_cmd(&src, None, None, false, false).expect("export writes");

        assert!(root.join("out.md").is_file(), "output belongs at the root");
        assert!(
            !sub.join("out.md").exists(),
            "output must not land beside its source"
        );
        assert!(
            !Path::new("out.md").exists(),
            "output must not land in the working directory"
        );

        // And `--check` agrees with the same file, which is what makes it
        // usable as a guard from anywhere.
        export_cmd(&src, None, None, false, true).expect("freshly exported output is up to date");

        let _ = fs::remove_dir_all(&root);
    }

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
                "::config(export: {{ type: commonmark, path: \"{}\" }})\n\n#[ Title ]\n",
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

    /// A directory with one document that cannot be exported, and one that can.
    fn dir_with_a_broken_document(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tomet_test_export_dir_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        fs::write(dir.join("default.config.tmt"), "::kind(config)\n").expect("config");
        fs::write(
            dir.join("good.tmt"),
            "::config(export: { type: commonmark, path: \"good.md\" })\n\n#[ Good ]\n",
        )
        .expect("good");
        fs::write(dir.join("broken.tmt"), "=[ unterminated\n").expect("broken");
        dir
    }

    /// A document that could not be exported is a document the run did not
    /// do. The run said so on stderr and still exited 0, so a script that
    /// exported a directory could not tell.
    #[test]
    fn a_directory_export_fails_when_a_document_could_not_be_exported() {
        let dir = dir_with_a_broken_document("run");
        let result = export_cmd(&dir, None, None, false, false);
        assert!(result.is_err(), "the broken document must fail the run");
        // The rest is still done: one bad document does not hide the others.
        assert!(dir.join("good.md").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    /// `--check` is a guard, and a guard that cannot read a document has not
    /// checked it. Passing there is the same as saying it is up to date.
    #[test]
    fn check_fails_when_a_document_could_not_be_checked() {
        let dir = dir_with_a_broken_document("check");
        export_cmd(&dir.join("good.tmt"), None, None, false, false).expect("exports");
        let result = export_cmd(&dir, None, None, false, true);
        assert!(
            result.is_err(),
            "a document that cannot be read must not pass --check"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_directory_of_good_documents_still_passes() {
        let dir = dir_with_a_broken_document("good");
        fs::remove_file(dir.join("broken.tmt")).unwrap();
        export_cmd(&dir, None, None, false, false).expect("exports");
        export_cmd(&dir, None, None, false, true).expect("is up to date");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_single_file_with_config_writes_output_file() {
        let temp_dir = std::env::temp_dir().join("tomet_test_export");
        let _ = fs::create_dir_all(&temp_dir);
        let src_file = temp_dir.join("test_doc.tmt");
        let out_file = temp_dir.join("test_out.md");

        let src_content = format!(
            "::config(\n  export: {{\n    type: commonmark\n    path: \"{}\"\n  }}\n)\n=[ Hello Export ]\n",
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
