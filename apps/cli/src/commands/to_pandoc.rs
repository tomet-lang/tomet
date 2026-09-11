use std::fs;
use std::path::PathBuf;

use crate::util::{format_parse_error, read, vault_for};

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
    let vault = vault_for(file);
    let (mut doc, _bindings) = vault
        .parse(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    vault
        .prepare(&mut doc, file)
        .map_err(|e| anyhow::anyhow!("{}: index query failed -- {e}", file.display()))?;
    let json = serde_json::to_string(&tomet_pandoc::to_pandoc(&doc))?;
    match out {
        Some(path) => fs::write(path, json)?,
        None => println!("{json}"),
    }
    Ok(())
}
