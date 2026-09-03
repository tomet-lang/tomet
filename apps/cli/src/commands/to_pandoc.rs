use std::fs;
use std::path::PathBuf;

use crate::util::{format_parse_error, read};

/// Writes a `.tmt` document as Pandoc's own AST, JSON-encoded.
///
/// Pandoc reads that on stdin with `-f json`, so this is the whole bridge
/// to every format it writes:
///
/// ```text
/// tomet to-pandoc doc.tmt | pandoc -f json -t docx -o doc.docx
/// ```
pub(crate) fn to_pandoc(file: &PathBuf, out: &Option<PathBuf>) -> anyhow::Result<()> {
    let src = read(file)?;
    let doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    let json = serde_json::to_string(&tomet_pandoc::to_pandoc(&doc))?;
    match out {
        Some(path) => fs::write(path, json)?,
        None => println!("{json}"),
    }
    Ok(())
}
