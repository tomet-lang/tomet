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

pub(crate) fn read(file: &PathBuf) -> anyhow::Result<String> {
    Ok(fs::read_to_string(file)?)
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
        let src = "<caution>[ unterminated";
        let formatted = format_parse_error(Path::new("test.tmt"), src, &err);
        assert!(formatted.contains("error: expected ']'"));
        assert!(formatted.contains("--> test.tmt:1:11"));
        assert!(formatted.contains("<caution>[ unterminated"));
        assert!(formatted.contains("^ expected ']'"));
    }
}
