use std::fs;
use std::path::PathBuf;

use crate::util::read;

/// Reads Pandoc's JSON AST and writes it out as a `.tmt` document.
///
/// The other half of the bridge -- everything Pandoc *reads* becomes
/// Tomet:
///
/// ```text
/// pandoc -t json report.docx | tomet from-pandoc -
/// ```
pub(crate) fn from_pandoc(file: &PathBuf, out: &Option<PathBuf>) -> anyhow::Result<()> {
    let json = read(file)?;
    let pandoc: tomet_pandoc::PandocDoc = serde_json::from_str(&json).map_err(|e| {
        anyhow::anyhow!(
            "not a Pandoc JSON document ({e}). Produce one with `pandoc -t json <file>`."
        )
    })?;
    let tmt = tomet_printer::document_to_tm(&tomet_pandoc::from_pandoc(&pandoc));
    match out {
        Some(path) => fs::write(path, tmt)?,
        None => print!("{tmt}"),
    }
    Ok(())
}
