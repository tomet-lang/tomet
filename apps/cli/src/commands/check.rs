use std::path::{Path, PathBuf};

use crate::util::{format_parse_error, read};

/// Checks one file, or every `.tmt` under a directory.
///
/// Checking is not parsing. This used to return OK the moment a document
/// parsed, so two `@meta`, or an element name that exists nowhere, passed
/// -- while the js/java/python bindings and `apps/web` all ran the
/// validator. Vocabularies have to be read from disk, which is why the
/// resolving lives here and not in the validator.
pub(crate) fn check(path: &PathBuf, data: bool, quiet: bool, json: bool) -> anyhow::Result<()> {
    if data {
        if !path.is_file() {
            return Err(anyhow::anyhow!(
                "--data takes one file, not a directory: {}",
                path.display()
            ));
        }
        let src = read(path)?;
        return match tomet_parser::parse_value(&src) {
            Ok(_) => {
                if !quiet {
                    println!("OK");
                }
                Ok(())
            }
            Err(err) => {
                report_parse_error(path, &src, &err, json);
                Err(anyhow::anyhow!("check failed"))
            }
        };
    }

    // Resolved once for the whole run. Per file it would re-read and
    // re-parse every declared vocabulary, which for this repository is
    // seven files times eighty-five documents.
    let (config, config_root) = tomet_config::find_config_file(path)
        .map(|(cfg, _, root)| (cfg, root))
        .unwrap_or_else(|| {
            let root = if path.is_dir() {
                path.clone()
            } else {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            };
            (tomet_config::PrinterConfig::default(), root)
        });

    let loaded = tomet_resolver::load_vocabularies(&config_root, &config.vocabularies);
    for problem in &loaded.errors {
        eprintln!("warning: {problem}");
    }

    let files = if path.is_file() {
        vec![path.clone()]
    } else if path.is_dir() {
        tomet_indexer::collect_tm_files_with_config(path, &config, &config_root)
    } else {
        return Err(anyhow::anyhow!("path '{}' does not exist", path.display()));
    };

    let mut checked = 0usize;
    let mut failed = 0usize;
    let mut warned = 0usize;
    let mut json_files: Vec<serde_json::Value> = Vec::new();

    for file in &files {
        let Ok(src) = std::fs::read_to_string(file) else {
            eprintln!("could not read {}", file.display());
            failed += 1;
            continue;
        };

        // Report and keep going, the convention `check_vault` and
        // `refactor --check` already follow: one broken file should not
        // hide the state of the rest.
        let doc = match tomet_parser::parse_document(&src) {
            Ok(doc) => doc,
            Err(err) => {
                if json {
                    json_files.push(serde_json::json!({
                        "file": file.display().to_string(),
                        "error": {
                            "message": err.message,
                            "line": err.line,
                            "column": err.column,
                            "offset": err.offset,
                        }
                    }));
                } else {
                    report_parse_error(file, &src, &err, false);
                }
                failed += 1;
                continue;
            }
        };

        let bindings = tomet_resolver::bindings_for(&doc, &loaded);
        let diagnostics = tomet_validator::validate_document_with(&doc, &bindings);
        checked += 1;
        if diagnostics.is_empty() {
            continue;
        }

        // A warning is the document telling you about itself -- `@draft`,
        // `@fixme`. It is reported and does not fail the run: marking a
        // gap has to stay cheaper than leaving it unmarked.
        let is_error =
            |d: &tomet_validator::Diagnostic| d.severity() == tomet_validator::Severity::Error;
        if diagnostics.iter().any(is_error) {
            failed += 1;
        }
        warned += diagnostics.iter().filter(|d| !is_error(d)).count();

        if json {
            json_files.push(serde_json::json!({
                "file": file.display().to_string(),
                "errors": diagnostics
                    .iter()
                    .map(|d| serde_json::json!({
                        "message": d.to_string(),
                        "severity": if is_error(d) { "error" } else { "warning" },
                    }))
                    .collect::<Vec<_>>(),
            }));
        } else {
            for d in &diagnostics {
                let label = if is_error(d) { "" } else { "warning: " };
                eprintln!("{}: {label}{d}", file.display());
            }
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&json_files)?);
    }

    if failed > 0 {
        return Err(anyhow::anyhow!(
            "{failed} of {} file(s) failed",
            files.len()
        ));
    }
    if !quiet {
        // The count is printed on a green run too. A document with gaps
        // still passes, and saying so is the only way the marks stay
        // visible rather than becoming decoration.
        let gaps = if warned > 0 {
            format!(" ({warned} warning(s))")
        } else {
            String::new()
        };
        if files.len() == 1 {
            println!("OK{gaps}");
        } else {
            println!("OK: {checked} file(s){gaps}");
        }
    }
    Ok(())
}

fn report_parse_error(file: &Path, src: &str, err: &tomet_parser::Error, json: bool) {
    if json {
        let value = serde_json::json!({
            "file": file.display().to_string(),
            "error": {
                "message": err.message,
                "line": err.line,
                "column": err.column,
                "offset": err.offset,
            }
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else {
        eprintln!("{}", format_parse_error(file, src, err));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn vault(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tomet_test_check_{name}_{}", nanoid::nanoid!()));
        fs::create_dir_all(dir.join(".tomet/vocabularies")).unwrap();
        fs::write(
            dir.join("default.config.tmt"),
            "@kind(config)\n@config(format:json)+++\n{ \"vocabularies\": [\".tomet/vocabularies/deck.vocabulary.tmt\"] }\n+++\n",
        )
        .unwrap();
        fs::write(
            dir.join(".tomet/vocabularies/deck.vocabulary.tmt"),
            "@kind(vocabulary)\n@vocabulary(deck)\n\n@element(card){}[ One card. ]\n",
        )
        .unwrap();
        dir
    }

    /// A directory is swept, and one bad file does not hide the rest --
    /// every failure is reported and the run ends non-zero.
    #[test]
    fn a_directory_is_swept_and_reports_every_failure() {
        let root = vault("sweep");
        fs::write(root.join("good.tmt"), "@kind(deck)\n\n@card{}\n").unwrap();
        fs::write(root.join("bad.tmt"), "@kind(deck)\n\n@nonesuch{}\n").unwrap();
        fs::write(
            root.join("worse.tmt"),
            "@kind(deck)\n@meta{a:1}\n@meta{b:2}\n",
        )
        .unwrap();

        let err = check(&root, false, true, false).expect_err("two files are bad");
        let message = err.to_string();
        assert!(message.contains("2 of"), "{message}");

        // And the good one alone passes, so the sweep is not failing
        // everything indiscriminately.
        check(&root.join("good.tmt"), false, true, false).expect("the good file passes");

        let _ = fs::remove_dir_all(&root);
    }

    /// The vault's `@kind` binding reaches every file in the sweep, not
    /// just the first -- the vocabularies are loaded once for the run.
    #[test]
    fn the_vocabulary_reaches_every_file_in_the_sweep() {
        let root = vault("shared");
        for name in ["a.tmt", "b.tmt", "c.tmt"] {
            fs::write(root.join(name), "@kind(deck)\n\n@card{}\n").unwrap();
        }
        check(&root, false, true, false).expect("all three resolve `card`");
        let _ = fs::remove_dir_all(&root);
    }

    /// `--data` parses one document with the value grammar; a directory
    /// of those is not a thing, so it says so rather than sweeping.
    #[test]
    fn data_mode_refuses_a_directory() {
        let root = vault("data");
        let err = check(&root, true, true, false).expect_err("a directory is refused");
        assert!(err.to_string().contains("--data takes one file"));
        let _ = fs::remove_dir_all(&root);
    }
}
