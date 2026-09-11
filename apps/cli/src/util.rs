use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn format_parse_error(path: &Path, src: &str, err: &tomet_parser::Error) -> String {
    let line_num = err.line;
    let col_num = err.column;

    let line_text = src.lines().nth(line_num.saturating_sub(1)).unwrap_or("");

    let indent = " ".repeat(col_num.saturating_sub(1));
    let line_str = line_num.to_string();
    let padding = " ".repeat(line_str.len());

    format!(
        "error: {}\n  --> {}:{}:{}\n   {}\n {} | {}\n   {}| {}^ {}",
        err.message,
        path.display(),
        line_num,
        col_num,
        "|",
        line_str,
        line_text,
        padding,
        indent,
        err.message
    )
}

/// Reads a source file, or standard input when the path is `-`.
///
/// The `-` convention is what makes this CLI usable in a pipe, which the
/// Pandoc bridge depends on: `pandoc -t json x.docx | tomet from-pandoc -`
/// has nowhere to put a temporary file. Nothing else changes -- a real
/// path is still read from disk.
pub(crate) fn read(file: &PathBuf) -> anyhow::Result<String> {
    if is_stdin(file) {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        return Ok(buf);
    }
    Ok(fs::read_to_string(file)?)
}

/// Whether a path means standard input rather than a file on disk.
pub(crate) fn is_stdin(path: &Path) -> bool {
    path.as_os_str() == "-"
}

/// The vault a command should read `file` through.
///
/// Standard input has no position of its own, so it borrows the working
/// directory's vault -- where the person running the pipe is standing.
/// Discovering from the literal path `-` would take its parent, the empty
/// path, as the vault root.
pub(crate) fn vault_for(file: &Path) -> tomet_load::Vault {
    if is_stdin(file) {
        tomet_load::Vault::discover(Path::new("."))
    } else {
        tomet_load::Vault::discover(file)
    }
}

/// Pulls a `title` string out of the document's `@meta` block, if it has
/// one -- `<title>`/`html`/`serve` prefer this over the filename when
/// present (see `.agents/tasks/ssg-readiness.md` step 1).
pub(crate) fn meta_title(doc: &tomet_ast::Document) -> Option<String> {
    let meta = tomet_semantics::document_meta(doc)?;
    let tomet_ast::Value::Map(map) = meta else {
        return None;
    };
    map.iter().find_map(|(k, v)| {
        if k != "title" {
            return None;
        }
        match v {
            tomet_ast::Value::String(s) => Some(s.clone()),
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_parse_error_with_source_context() {
        let err = tomet_parser::Error {
            message: "expected ']'".into(),
            line: 1,
            column: 11,
            offset: 10,
        };
        let src = "@caution[ unterminated";
        let formatted = format_parse_error(Path::new("test.tmt"), src, &err);
        assert!(formatted.contains("error: expected ']'"));
        assert!(formatted.contains("--> test.tmt:1:11"));
        assert!(formatted.contains("@caution[ unterminated"));
        assert!(formatted.contains("^ expected ']'"));
    }
}
