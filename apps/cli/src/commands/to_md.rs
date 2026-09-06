use std::fs;
use std::path::PathBuf;

use crate::util::{format_parse_error, read, render_context};

pub(crate) fn to_md(file: &PathBuf, out: &Option<PathBuf>) -> anyhow::Result<()> {
    let src = read(file)?;
    let doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    // The same context `export` builds, so the two cannot disagree about
    // what a `${macro.…}` or a `${self.…}` means.
    let project_root = tomet_config::find_config_file(file)
        .map(|(_, _, root)| root)
        .unwrap_or_else(|| {
            file.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
    let (config, vars) = render_context(&doc, file, &project_root);
    let markdown = tomet_markdown::to_markdown_with_context(&doc, &config, &vars);
    match out {
        Some(path) => fs::write(path, markdown)?,
        None => println!("{markdown}"),
    }
    Ok(())
}
