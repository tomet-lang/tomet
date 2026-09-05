use std::path::{Path, PathBuf};

use crate::util::{format_parse_error, read};

pub(crate) fn check(file: &PathBuf, data: bool, quiet: bool, json: bool) -> anyhow::Result<()> {
    let src = read(file)?;

    if data {
        return match tomet_parser::parse_value(&src) {
            Ok(_) => {
                if !quiet {
                    println!("OK");
                }
                Ok(())
            }
            Err(err) => report_parse_error(file, &src, &err, json),
        };
    }

    let doc = match tomet_parser::parse_document(&src) {
        Ok(doc) => doc,
        Err(err) => return report_parse_error(file, &src, &err, json),
    };

    // Parsing is not checking. Until now this returned OK the moment the
    // document parsed, so a document with two `@meta`, or an element name
    // that exists nowhere, passed -- while the js/java/python bindings and
    // `apps/web` all ran the validator. The vocabularies a document has in
    // scope have to be read from disk, which is why this lives here and
    // not in the validator.
    let (config, config_root) = tomet_config::find_config_file(file)
        .map(|(cfg, _, root)| (cfg, root))
        .unwrap_or_else(|| {
            (
                tomet_config::config_for(Some(file), &src),
                file.parent().unwrap_or(Path::new(".")).to_path_buf(),
            )
        });

    let loaded = tomet_resolver::load_vocabularies(&config_root, &config.vocabularies);
    for problem in &loaded.errors {
        eprintln!("warning: {problem}");
    }

    let bindings = tomet_resolver::bindings_for(&doc, &loaded);
    let errors = tomet_validator::validate_document_with(&doc, &bindings);

    if errors.is_empty() {
        if !quiet {
            println!("OK");
        }
        return Ok(());
    }

    if json {
        let items: Vec<serde_json::Value> = errors
            .iter()
            .map(|e| serde_json::json!({ "message": e.to_string() }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({ "file": file.display().to_string(), "errors": items })
            )?
        );
    } else {
        for e in &errors {
            eprintln!("{}: {e}", file.display());
        }
    }
    Err(anyhow::anyhow!("check failed"))
}

fn report_parse_error(
    file: &Path,
    src: &str,
    err: &tomet_parser::Error,
    json: bool,
) -> anyhow::Result<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "file": file.display().to_string(),
                "error": {
                    "message": err.message,
                    "line": err.line,
                    "column": err.column,
                    "offset": err.offset,
                }
            }))?
        );
    } else {
        eprintln!("{}", format_parse_error(file, src, err));
    }
    Err(anyhow::anyhow!("check failed"))
}
