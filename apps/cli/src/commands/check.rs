use std::path::PathBuf;

use crate::util::{format_parse_error, read};

pub(crate) fn check(file: &PathBuf, data: bool, quiet: bool, json: bool) -> anyhow::Result<()> {
    let src = read(file)?;
    let res = if data {
        tomet_parser::parse_value(&src).map(|_| ())
    } else {
        tomet_parser::parse_document(&src).map(|_| ())
    };

    match res {
        Ok(()) => {
            if !quiet {
                println!("OK");
            }
            Ok(())
        }
        Err(err) => {
            if json {
                let json_err = serde_json::json!({
                    "file": file.display().to_string(),
                    "error": {
                        "message": err.message,
                        "line": err.line,
                        "column": err.column,
                        "offset": err.offset,
                    }
                });
                println!("{}", serde_json::to_string_pretty(&json_err)?);
            } else {
                eprintln!("{}", format_parse_error(file, &src, &err));
            }
            Err(anyhow::anyhow!("check failed"))
        }
    }
}
