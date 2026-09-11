use std::fs;
use std::path::PathBuf;

use crate::util::{format_parse_error, read, vault_for};

pub(crate) fn to_md(file: &PathBuf, out: &Option<PathBuf>) -> anyhow::Result<()> {
    let src = read(file)?;
    let vault = vault_for(file);
    let (mut doc, _bindings) = vault
        .parse(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    // The vault settles what a `${macro.…}` or a `${self.…}` means, so
    // this and `export` cannot disagree about it -- and neither can the
    // other three output formats, which is what they used to do.
    vault
        .prepare(&mut doc, file)
        .map_err(|e| anyhow::anyhow!("{}: index query failed -- {e}", file.display()))?;
    let markdown = tomet_markdown::to_markdown(&doc);
    match out {
        Some(path) => fs::write(path, markdown)?,
        None => println!("{markdown}"),
    }
    Ok(())
}
