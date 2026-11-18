use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use tomet_load::Vault;
use tomet_semantics::{ExportDestination, ExportTarget, ExportType, document_config};

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
        scope: ExportScope::SingleFile {
            target_file: file_path,
            override_out,
        },
    };
    match export_one(&settings, file_path) {
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

/// Distinguishes a single-file export run from a directory export run.
///
/// In a single-file export, `--out` applies only to the file being exported,
/// while any other document reached via `tmt:` is located by its own declared path.
/// In a directory export, `--out` relocates all documents under the directory together.
#[derive(Clone, Copy)]
pub(crate) enum ExportScope<'a> {
    SingleFile {
        target_file: &'a Path,
        override_out: Option<&'a Path>,
    },
    Directory {
        dir_path: &'a Path,
        override_out: Option<&'a Path>,
    },
}

/// What stays the same for every file of one export run.
#[derive(Clone, Copy)]
pub(crate) struct ExportSettings<'a> {
    pub(crate) override_type: Option<&'a str>,
    pub(crate) advanced: bool,
    pub(crate) check: bool,
    pub(crate) project_root: &'a Path,
    pub(crate) vault: &'a Vault,
    pub(crate) scope: ExportScope<'a>,
}

/// Determines which formats a document is exported to and where each is written,
/// according to the document's `::export` configuration and the run's settings.
///
/// Returns pairs of `(ExportType, Option<PathBuf>)`. A destination of `None` indicates
/// that output goes to stdout (when a single format is exported and no output path is declared
/// or overridden).
///
/// Output destination rules for `--out`:
/// - In a directory export:
///   - Documents outside the directory being exported are not written in this run (returns empty).
///   - When `--out DIR` is provided, every document under the directory is placed under `DIR`,
///     preserving its subdirectory structure: `DIR/<parent>/<stem>.<ext>`.
/// - In a single-file export:
///   - `--out` applies only to the single file being exported. Other documents reached via `tmt:`
///     follow their own declared `::export` paths or fallback.
///   - If multiple formats are exported, or if `--out` points to an existing directory or ends with
///     a trailing slash (`/` or `\`), `--out` is treated as a directory: `DIR/<stem>.<ext>`.
///   - Otherwise, `--out` is treated as the output file path.
/// - Fallback when `--out` does not apply:
///   - If the document declares an export path in `::export`, it is resolved relative to the project root.
///   - Otherwise, if exactly one format is exported, output goes to stdout (`None`).
///   - If multiple formats are exported without declared paths, each is written to `{stem}.{ext}`.
pub(crate) fn document_export_destinations(
    doc: &tomet_ast::File,
    file_path: &Path,
    settings: &ExportSettings<'_>,
) -> Vec<(ExportType, Option<PathBuf>)> {
    let file_doc_ref = settings.vault.doc_ref(file_path);

    // Compute scope-specific directory/target info once.
    // In a directory export, any document outside the directory being exported
    // is not written in this run, so its destination is omitted and links to it remain unchanged.
    let (dir_out_parent, is_single_target) = match settings.scope {
        ExportScope::Directory {
            dir_path,
            override_out,
        } => {
            let dir_doc_ref = settings.vault.doc_ref(dir_path);
            let is_under = dir_doc_ref.0.is_empty()
                || file_doc_ref.0 == dir_doc_ref.0
                || file_doc_ref.0.starts_with(&format!("{}/", dir_doc_ref.0));
            if !is_under {
                return Vec::new();
            }
            let parent = override_out.map(|_| {
                let rel_str = if dir_doc_ref.0.is_empty() {
                    file_doc_ref.0.as_str()
                } else if file_doc_ref.0 == dir_doc_ref.0 {
                    ""
                } else {
                    &file_doc_ref.0[dir_doc_ref.0.len() + 1..]
                };
                let rel_path = Path::new(rel_str);
                rel_path
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf()
            });
            (parent, false)
        }
        ExportScope::SingleFile {
            target_file,
            override_out,
        } => {
            let is_target =
                override_out.is_some() && file_doc_ref.0 == settings.vault.doc_ref(target_file).0;
            (None, is_target)
        }
    };

    let config = document_config(doc);

    // Determine target formats to export
    let targets = if let Some(t_str) = settings.override_type {
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

    let targets_len = targets.len();
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    targets
        .into_iter()
        .map(move |target| {
            let ext = match &target {
                ExportType::CommonMark => "md",
                ExportType::Html => "html",
                ExportType::Typst => "typ",
                ExportType::Pandoc => "json",
                ExportType::Custom(s) => s.as_str(),
            };

            // Scope decides only the destination that `--out` imposes (if any)
            let out_override = match settings.scope {
                ExportScope::Directory {
                    override_out: Some(out_dir),
                    ..
                } => {
                    let parent = dir_out_parent.as_deref().unwrap_or_else(|| Path::new(""));
                    Some(out_dir.join(parent).join(format!("{stem}.{ext}")))
                }
                ExportScope::SingleFile {
                    override_out: Some(out),
                    ..
                } if is_single_target => {
                    if out.is_dir()
                        || out.to_string_lossy().ends_with('/')
                        || out.to_string_lossy().ends_with('\\')
                        || targets_len > 1
                    {
                        Some(out.join(format!("{stem}.{ext}")))
                    } else {
                        Some(out.to_path_buf())
                    }
                }
                _ => None,
            };

            // Shared fallback when `--out` does not impose a destination
            let out_path = if let Some(p) = out_override {
                Some(p)
            } else if let Some(cfg_path) = config.export_path_for(&target) {
                Some(tomet_indexer::resolve_document_relative(
                    file_path,
                    cfg_path,
                    settings.project_root,
                ))
            } else if targets_len == 1 {
                None
            } else {
                Some(PathBuf::from(format!("{stem}.{ext}")))
            };

            (target, out_path)
        })
        .collect()
}

/// Categorizes the destination for export link rewriting.
///
/// Determines whether `out_path` is a destination within the vault root,
/// outside the vault root, or absent (stdout).
fn export_destination_for(
    out_path: Option<&Path>,
    vault_root: &Path,
) -> ExportDestination<'static> {
    let Some(dest) = out_path else {
        return ExportDestination::Stdout;
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let abs_root = if vault_root.is_absolute() {
        vault_root.to_path_buf()
    } else {
        cwd.join(vault_root)
    };
    let abs_dest = if dest.is_absolute() {
        dest.to_path_buf()
    } else {
        cwd.join(dest)
    };
    let norm_root = tomet_indexer::normalize_join(&abs_root, Path::new(""));
    let norm_dest = tomet_indexer::normalize_join(&abs_dest, Path::new(""));
    let norm_dest_str = norm_dest.to_string_lossy().replace('\\', "/");

    if norm_dest.starts_with(&norm_root) {
        ExportDestination::InsideVault(std::borrow::Cow::Owned(norm_dest_str))
    } else {
        ExportDestination::OutsideVault(std::borrow::Cow::Owned(norm_dest_str))
    }
}

fn export_one(settings: &ExportSettings, file_path: &Path) -> anyhow::Result<OneFileExport> {
    let ExportSettings {
        advanced,
        check,
        vault,
        ..
    } = *settings;

    let src = fs::read_to_string(file_path)?;
    // `vault.parse` and not `vault.document`, so the source text is still
    // in hand for `format_parse_error`'s pointer-at-the-column report.
    let (mut doc, bindings) = vault
        .parse(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file_path, &src, &e)))?;

    // Every `{$ ... }` is resolved before anything renders, so every converter
    // is handed its value and none of them needs to know expressions exist.
    vault.prepare(&mut doc, file_path);

    let destinations = document_export_destinations(&doc, file_path, settings);

    let mut out = OneFileExport {
        stdout_lines: Vec::new(),
        stdout_raw: Vec::new(),
        stale_paths: Vec::new(),
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    for (target, out_path) in destinations {
        // Under `--check`, if there is no destination output path, there is no
        // artifact on disk to compare against. Skip rendering entirely.
        if check && out_path.is_none() {
            continue;
        }

        let mut render_doc = doc.clone();
        let export_dest = export_destination_for(out_path.as_deref(), &vault.root);
        tomet_semantics::rewrite_export_links(&mut render_doc, export_dest, |vault_path| {
            let Ok(target_file) = vault.document_path(vault_path) else {
                // Not a document of this vault; left as written.
                return None;
            };
            let Ok((target_doc, _)) = vault.file(&target_file) else {
                // Target file does not exist on disk in the vault; left as written.
                return None;
            };
            let target_dests = document_export_destinations(&target_doc, &target_file, settings);
            for (fmt, path) in target_dests {
                if let (true, Some(p)) = (fmt == target, path) {
                    let abs_p = if p.is_absolute() { p } else { cwd.join(p) };
                    let norm_p = tomet_indexer::normalize_join(&abs_p, Path::new(""));
                    return Some(ExportTarget::Exported(
                        norm_p.to_string_lossy().replace('\\', "/"),
                    ));
                }
            }
            let abs_target = if target_file.is_absolute() {
                target_file
            } else {
                cwd.join(&target_file)
            };
            let norm_target = tomet_indexer::normalize_join(&abs_target, Path::new(""));
            Some(ExportTarget::Source(
                norm_target.to_string_lossy().replace('\\', "/"),
            ))
        });

        let rendered = match target {
            ExportType::CommonMark => tomet_markdown::to_markdown(&render_doc, &bindings),
            // Pandoc's AST, not a rendering -- `pandoc -f json` turns it
            // into whatever format is actually wanted.
            ExportType::Pandoc => {
                serde_json::to_string(&tomet_pandoc::to_pandoc(&render_doc, &bindings))?
            }
            ExportType::Html => {
                let filename_title = file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Tomet");
                let title = meta_title(&render_doc).unwrap_or_else(|| filename_title.to_string());
                let options = tomet_html::RenderOptions {
                    number_headings: advanced,
                    auto_slug_headings: advanced,
                    lang: None,
                    ..Default::default()
                };
                tomet_html::render_page_with(&render_doc, &bindings, &title, &options)
            }
            ExportType::Typst => tomet_typst::to_typst(&render_doc, &bindings),
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
        scope: ExportScope::Directory {
            dir_path,
            override_out,
        },
    };

    // Phase 1: Process documents in parallel governed by jobserver tokens
    let results: Vec<Result<OneFileExport, String>> = files
        .par_iter()
        .map(|file| {
            let _token = jobserver_gate::acquire();
            export_one(&settings, file).map_err(|e| e.to_string())
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
    /// directory, are the two ways this can go wrong; both are
    /// asserted against. `tmtroot/` depends on this: its whole point is
    /// that a source in one place produces a file in another.
    #[test]
    fn a_bare_export_path_is_measured_from_the_project_root() {
        let root = std::env::temp_dir().join("tomet_test_export_root");
        let _ = fs::remove_dir_all(&root);
        let sub = root.join("sub");
        fs::create_dir_all(&sub).expect("temp dirs");
        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("config");

        let src = sub.join("doc.tmt");
        fs::write(
            &src,
            "::export(type: commonmark, path: \"out.md\")\n\n#[ Title ]\n",
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
                "::export(type: commonmark, path: \"{}\")\n\n#[ Title ]\n",
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
        fs::write(
            dir.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("config");
        fs::write(
            dir.join("good.tmt"),
            "::export(type: commonmark, path: \"good.md\")\n\n#[ Good ]\n",
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
            "::export(\n  type: commonmark\n  path: \"{}\"\n)\n=[ Hello Export ]\n",
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

    #[test]
    fn language_bar_and_relative_links_end_to_end() {
        let root = std::env::temp_dir().join(format!("tomet_test_lang_bar_{}", nanoid::nanoid!()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("locales/ja")).expect("create dirs");
        fs::create_dir_all(root.join("locales/zh")).expect("create dirs");
        fs::create_dir_all(root.join("docs")).expect("create dirs");

        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n::field(lang: (type: string), lang_name: (type: string))\n",
        )
        .expect("write config");

        let bar_expr = "{$ vault::docs\n    \
                .filter(d => d::filename == \"readme.tmt\")\n    \
                .sort(d => d::meta.lang)\n    \
                .map(d => d.link(d::meta.lang_name))\n    \
                .join(\" | \") }\n\n";

        fs::write(
            root.join("readme.tmt"),
            format!(
                "::export(type: commonmark, path: \"out/README.md\")\n\
                 ::meta(lang: \"en\", lang_name: \"English\")\n\n\
                 {bar_expr}\
                 See also @link(tmt:plain.tmt)[No Export Doc].\n"
            ),
        )
        .expect("write root readme");

        fs::write(
            root.join("locales/ja/readme.tmt"),
            format!(
                "::export(type: commonmark, path: \"out/README.ja.md\")\n\
                 ::meta(lang: \"ja\", lang_name: \"Japanese\")\n\n\
                 {bar_expr}\
                 #[ Japanese Readme ]\n"
            ),
        )
        .expect("write ja readme");

        fs::write(
            root.join("locales/zh/readme.tmt"),
            format!(
                "::export(type: commonmark, path: \"out/README.zh-CN.md\")\n\
                 ::meta(lang: \"zh\", lang_name: \"Chinese\")\n\n\
                 {bar_expr}\
                 #[ Chinese Readme ]\n"
            ),
        )
        .expect("write zh readme");

        fs::write(
            root.join("docs/note.tmt"),
            "::export(type: commonmark, path: \"out/docs/note.md\")\n\n\
             Back to @link(tmt:readme.tmt)[Root Readme].\n",
        )
        .expect("write docs note");

        fs::write(
            root.join("plain.tmt"),
            "#[ Plain Document without export ]\n",
        )
        .expect("write plain doc");

        // Real export run
        export_cmd(&root, None, None, false, false).expect("export succeeds");

        let readme_out =
            fs::read_to_string(root.join("out/README.md")).expect("read out/README.md");
        // In English readme, bar lists all three languages including English itself
        assert!(
            readme_out.contains("[English](README.md)"),
            "expected [English](README.md) in: {readme_out}"
        );
        assert!(
            readme_out.contains("[Japanese](README.ja.md)"),
            "expected [Japanese](README.ja.md) in: {readme_out}"
        );
        assert!(
            readme_out.contains("[Chinese](README.zh-CN.md)"),
            "expected [Chinese](README.zh-CN.md) in: {readme_out}"
        );
        // Target without ::export points at its source file relative to the document's output directory
        assert!(
            readme_out.contains("[No Export Doc](../plain.tmt)"),
            "expected [No Export Doc](../plain.tmt) in: {readme_out}"
        );

        let ja_out =
            fs::read_to_string(root.join("out/README.ja.md")).expect("read out/README.ja.md");
        // In Japanese readme, bar lists Japanese itself with its own file name README.ja.md
        assert!(
            ja_out.contains("[English](README.md)"),
            "expected [English](README.md) in: {ja_out}"
        );
        assert!(
            ja_out.contains("[Japanese](README.ja.md)"),
            "expected [Japanese](README.ja.md) in: {ja_out}"
        );
        assert!(
            ja_out.contains("[Chinese](README.zh-CN.md)"),
            "expected [Chinese](README.zh-CN.md) in: {ja_out}"
        );

        let zh_out =
            fs::read_to_string(root.join("out/README.zh-CN.md")).expect("read out/README.zh-CN.md");
        // In Chinese readme, bar lists Chinese itself with its own file name README.zh-CN.md
        assert!(
            zh_out.contains("[English](README.md)"),
            "expected [English](README.md) in: {zh_out}"
        );
        assert!(
            zh_out.contains("[Japanese](README.ja.md)"),
            "expected [Japanese](README.ja.md) in: {zh_out}"
        );
        assert!(
            zh_out.contains("[Chinese](README.zh-CN.md)"),
            "expected [Chinese](README.zh-CN.md) in: {zh_out}"
        );

        let note_out =
            fs::read_to_string(root.join("out/docs/note.md")).expect("read out/docs/note.md");
        // Document in subdirectory gets ../ relative link to root
        assert!(
            note_out.contains("[Root Readme](../README.md)"),
            "expected [Root Readme](../README.md) in: {note_out}"
        );

        // Export check agrees with real export
        export_cmd(&root, None, None, false, true).expect("--check passes on fresh export");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn html_and_commonmark_in_one_run_give_right_extension() {
        let root = std::env::temp_dir().join(format!("tomet_test_multi_fmt_{}", nanoid::nanoid!()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create dir");

        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("write config");

        fs::write(
            root.join("source.tmt"),
            "::export(\n  \
               type: (commonmark, html)\n  \
               path: (commonmark: \"out/source.md\", html: \"out/source.html\")\n\
             )\n\n\
             See @link(tmt:target.tmt)[The Target].\n",
        )
        .expect("write source");

        fs::write(
            root.join("target.tmt"),
            "::export(\n  \
               type: (commonmark, html)\n  \
               path: (commonmark: \"out/target.md\", html: \"out/target.html\")\n\
             )\n\n\
             #[ Target Content ]\n",
        )
        .expect("write target");

        export_cmd(&root, None, None, false, false).expect("export multi formats");

        let md_out = fs::read_to_string(root.join("out/source.md")).expect("read source.md");
        assert!(
            md_out.contains("[The Target](target.md)"),
            "expected [The Target](target.md) in: {md_out}"
        );

        let html_out = fs::read_to_string(root.join("out/source.html")).expect("read source.html");
        assert!(
            html_out.contains("href=\"target.html\""),
            "expected href=\"target.html\" in: {html_out}"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn directory_export_with_override_out_relocates_together() {
        let root = std::env::temp_dir().join(format!("tomet_test_dir_out_{}", nanoid::nanoid!()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/sub")).expect("create dirs");
        let dest = root.join("dist");

        fs::write(
            root.join("src").join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("write config");

        fs::write(
            root.join("src/root.tmt"),
            "See @link(tmt:sub/child.tmt)[Child].\n",
        )
        .expect("write root doc");

        fs::write(
            root.join("src/sub/child.tmt"),
            "Back to @link(tmt:root.tmt)[Root].\n",
        )
        .expect("write child doc");

        export_cmd(&root.join("src"), None, Some(&dest), false, false)
            .expect("export directory with --out");

        let root_out = fs::read_to_string(dest.join("root.md")).expect("read dist/root.md");
        assert!(
            root_out.contains("[Child](sub/child.md)"),
            "expected [Child](sub/child.md) in: {root_out}"
        );

        let child_out =
            fs::read_to_string(dest.join("sub/child.md")).expect("read dist/sub/child.md");
        assert!(
            child_out.contains("[Root](../root.md)"),
            "expected [Root](../root.md) in: {child_out}"
        );

        export_cmd(&root.join("src"), None, Some(&dest), false, true)
            .expect("--check passes with --out");

        let _ = fs::remove_dir_all(&root);
    }

    /// In a directory export, a `tmt:` link to a document outside the exported
    /// directory is not written in this run, so it must keep its vault-relative source path
    /// both with and without `--out`.
    #[test]
    fn directory_export_target_outside_dir_is_not_rewritten() {
        let root = std::env::temp_dir().join(format!("tomet_test_f6_{}", nanoid::nanoid!()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs")).expect("create docs dir");

        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("write config");

        fs::write(
            root.join("readme.tmt"),
            "::export(type: commonmark, path: \"out/README.md\")\n\n#[ Root Readme ]\n",
        )
        .expect("write root readme");

        fs::write(
            root.join("docs/guide.tmt"),
            "::export(type: commonmark, path: \"out/docs/guide.md\")\n\n\
             See @link(tmt:readme.tmt)[Root Readme].\n",
        )
        .expect("write guide");

        // 1. Directory export of "docs" without --out (destination inside vault root points to target source file)
        export_cmd(&root.join("docs"), None, None, false, false)
            .expect("export docs without --out");
        let guide_out = fs::read_to_string(root.join("out/docs/guide.md")).expect("read guide");
        assert!(
            guide_out.contains("[Root Readme](../../readme.tmt)"),
            "expected [Root Readme](../../readme.tmt) in: {guide_out}"
        );

        // 2. Directory export of "docs" with --out outside the vault root keeps target as written
        let dist = std::env::temp_dir().join(format!("tomet_test_f6_dist_{}", nanoid::nanoid!()));
        let _ = fs::remove_dir_all(&dist);
        export_cmd(&root.join("docs"), None, Some(&dist), false, false)
            .expect("export docs with --out");
        let dist_guide = fs::read_to_string(dist.join("guide.md")).expect("read dist guide");
        assert!(
            dist_guide.contains("[Root Readme](readme.tmt)"),
            "expected [Root Readme](readme.tmt) in: {dist_guide}"
        );
        let _ = fs::remove_dir_all(&dist);

        let _ = fs::remove_dir_all(&root);
    }

    /// Verifies `--out` behavior across single-file and directory exports.
    #[test]
    fn single_file_and_directory_out_behaviors() {
        let root = std::env::temp_dir().join(format!("tomet_test_f5_{}", nanoid::nanoid!()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create dir");

        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("write config");

        let doc_src = root.join("doc.tmt");
        fs::write(
            &doc_src,
            "::export(type: commonmark)\n\n#[ Hello Document ]\n",
        )
        .expect("write doc.tmt");

        // Case 1: Single file with trailing slash -> treated as directory
        let out_slash_dir = root.join("slash_out");
        let slash_str = format!("{}/", out_slash_dir.display());
        let slash_path = Path::new(&slash_str);
        export_cmd(&doc_src, None, Some(slash_path), false, false).expect("export trailing slash");
        assert!(out_slash_dir.join("doc.md").is_file());

        // Case 2: Single file with existing directory
        let existing_dir = root.join("existing_dir");
        fs::create_dir_all(&existing_dir).expect("create existing dir");
        export_cmd(&doc_src, None, Some(&existing_dir), false, false).expect("export existing dir");
        assert!(existing_dir.join("doc.md").is_file());

        // Case 3: Single file with custom file path
        let custom_file = root.join("custom_dest.md");
        export_cmd(&doc_src, None, Some(&custom_file), false, false).expect("export file path");
        assert!(custom_file.is_file());

        // Case 4: Single file with multiple formats -> treated as directory
        let multi_src = root.join("multi.tmt");
        fs::write(
            &multi_src,
            "::export(type: (commonmark, html))\n\n#[ Multi Document ]\n",
        )
        .expect("write multi.tmt");
        let multi_dir = root.join("multi_out");
        export_cmd(&multi_src, None, Some(&multi_dir), false, false).expect("export multi format");
        assert!(multi_dir.join("multi.md").is_file());
        assert!(multi_dir.join("multi.html").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    /// A document in a subdirectory linking to a non-exported document
    /// rewrites the link to point at the target's source file relative to the output directory.
    #[test]
    fn subdirectory_document_linking_to_non_exported_document() {
        let root = std::env::temp_dir().join(format!("tomet_test_sub_unexp_{}", nanoid::nanoid!()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs")).expect("create dirs");
        fs::create_dir_all(root.join("tmtroot/why")).expect("create dirs");

        fs::write(
            root.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("write config");

        fs::write(
            root.join("tmtroot/why/root.tmt"),
            "#[ Why Tomet ]\n\nExplanation text.\n",
        )
        .expect("write why doc");

        fs::write(
            root.join("docs/quick_start.tmt"),
            "::export(type: commonmark, path: \"docs/quick_start.md\")\n\n\
             See @link(tmt:tmtroot/why/root.tmt)[Why Tomet].\n",
        )
        .expect("write quick start");

        export_cmd(&root.join("docs/quick_start.tmt"), None, None, false, false)
            .expect("export succeeds");

        let qs_out =
            fs::read_to_string(root.join("docs/quick_start.md")).expect("read quick_start.md");
        assert!(
            qs_out.contains("[Why Tomet](../tmtroot/why/root.tmt)"),
            "expected [Why Tomet](../tmtroot/why/root.tmt) in: {qs_out}"
        );

        export_cmd(&root.join("docs/quick_start.tmt"), None, None, false, true)
            .expect("--check passes on fresh export");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn export_link_lookup_stays_inside_the_vault_regardless_of_working_directory() {
        use std::process::Command;

        let base = std::env::temp_dir().join(format!(
            "tomet_test_export_vault_boundary_{}",
            nanoid::nanoid!()
        ));
        let _ = fs::remove_dir_all(&base);
        let vault = base.join("vault");
        let outside_dir = base.join("outside_dir");
        fs::create_dir_all(vault.join("sub")).expect("create vault dirs");
        fs::create_dir_all(&outside_dir).expect("create outside dir");

        let outside_doc = base.join("outside.tmt");
        fs::write(&outside_doc, "::kind(doc)\n#[ Outside Doc ]\n").expect("write outside doc");
        let outside_txt = base.join("outside-secret.txt");
        fs::write(&outside_txt, "secret\n").expect("write outside txt");

        fs::write(
            vault.join(tomet_load_config::VAULT_CONFIG_FILE),
            "::kind(config)\n",
        )
        .expect("write config");
        fs::write(vault.join("data.txt"), "inside-txt\n").expect("write data.txt");
        fs::write(vault.join("sub/ex.tmt"), "::kind(doc)\n#[ Sub Ex ]\n")
            .expect("write sub/ex.tmt");

        #[cfg(unix)]
        let has_symlink = {
            let sym_file = vault.join("sym_out.tmt");
            std::os::unix::fs::symlink(&outside_doc, &sym_file).is_ok()
        };
        #[cfg(not(unix))]
        let has_symlink = false;

        let link_body = if has_symlink {
            "@link(tmt:../outside.tmt)[outside-doc] @link(tmt:../outside-secret.txt)[outside-txt] @link(tmt:data.txt)[inside-txt] @link(tmt:sym_out.tmt)[sym-doc] @link(tmt:sub/ex.tmt)[ok]\n"
        } else {
            "@link(tmt:../outside.tmt)[outside-doc] @link(tmt:../outside-secret.txt)[outside-txt] @link(tmt:data.txt)[inside-txt] @link(tmt:sub/ex.tmt)[ok]\n"
        };

        fs::write(
            vault.join("l.tmt"),
            format!("::export(type: commonmark, path: \"out/l.md\")\n\n{link_body}"),
        )
        .expect("write l.tmt");

        let bin = option_env!("CARGO_BIN_EXE_tomet")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("tomet")
            });

        // Run 1: from the vault directory
        let out1 = Command::new(&bin)
            .current_dir(&vault)
            .args(["export", "l.tmt"])
            .output()
            .expect("run export from vault dir");
        assert!(
            out1.status.success(),
            "export from vault dir failed: {}",
            String::from_utf8_lossy(&out1.stderr)
        );
        let content1 =
            fs::read_to_string(vault.join("out/l.md")).expect("read out/l.md after run 1");

        assert!(content1.contains("[outside-doc](../outside.tmt)"));
        assert!(content1.contains("[outside-txt](../outside-secret.txt)"));
        assert!(content1.contains("[inside-txt](data.txt)"));
        if has_symlink {
            assert!(content1.contains("[sym-doc](sym_out.tmt)"));
        }
        assert!(content1.contains("[ok](../sub/ex.tmt)"));

        // Remove out/l.md before run 2
        fs::remove_file(vault.join("out/l.md")).expect("remove out/l.md");

        // Run 2: from another working directory
        let l_tmt_path = vault.join("l.tmt");
        let out2 = Command::new(&bin)
            .current_dir(&outside_dir)
            .args(["export", l_tmt_path.to_str().unwrap()])
            .output()
            .expect("run export from outside dir");
        assert!(
            out2.status.success(),
            "export from outside dir failed: {}",
            String::from_utf8_lossy(&out2.stderr)
        );
        let content2 =
            fs::read_to_string(vault.join("out/l.md")).expect("read out/l.md after run 2");

        assert_eq!(
            content1, content2,
            "export output must not depend on working directory"
        );

        let _ = fs::remove_dir_all(&base);
    }
}
