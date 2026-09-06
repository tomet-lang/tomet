use std::fs;
use std::path::{Path, PathBuf};

use crate::util::{format_parse_error, meta_title};

/// Shared by the `html` command and the `serve` command's per-request
/// re-render (`commands::serve::render_handler`).
///
/// `body` drops the `<html>`/`<head>` shell and returns the rendered body
/// alone, for callers that supply their own page -- a static-site
/// generator templating the fragment into its own layout, say. The shell
/// is what carries `lang` and the document title, so both are ignored in
/// that mode.
pub(crate) fn render_file(
    file: &Path,
    advanced: bool,
    lang: Option<String>,
    body: bool,
) -> anyhow::Result<String> {
    let src = fs::read_to_string(file)?;
    let doc = tomet_parser::parse_document(&src)
        .map_err(|e| anyhow::anyhow!("{}", format_parse_error(file, &src, &e)))?;
    let options = tomet_html::RenderOptions {
        number_headings: advanced,
        auto_slug_headings: advanced,
        lang,
    };
    if body {
        return Ok(tomet_html::render_body_with(&doc, &options));
    }
    let filename_title = file.file_stem().and_then(|s| s.to_str()).unwrap_or("Tomet");
    let title = meta_title(&doc).unwrap_or_else(|| filename_title.to_string());
    Ok(tomet_html::render_page_with(&doc, &title, &options))
}

pub(crate) fn html(
    file: &PathBuf,
    out: &Option<PathBuf>,
    advanced: bool,
    lang: Option<String>,
    body: bool,
) -> anyhow::Result<()> {
    let page = render_file(file, advanced, lang, body)?;
    match out {
        Some(path) => fs::write(path, page)?,
        None => println!("{page}"),
    }
    Ok(())
}
