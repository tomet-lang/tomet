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

/// The config and variables a `${...}` needs, for a document being
/// rendered from a real file inside a real vault.
///
/// Two things the pure renderers cannot know on their own:
///
/// - **The vault's macros.** `tomet_semantics::document_config` reads a
///   document's own `@config` and nothing else, so a `default.config.tmt`
///   defining `gh`, `wiki` and a dozen others expanded nowhere. They are
///   merged underneath the document's own, which wins on a clash --
///   nearer declaration, same rule as the config search itself.
/// - **Where the document is.** `self.path` is relative to the project
///   root (`tmtroot/agents.tmt`), `self.filename` is the basename. Two
///   fields rather than one, because naming either for the other's
///   meaning is how a field starts lying.
pub(crate) fn render_context(
    doc: &tomet_ast::Document,
    file_path: &Path,
    project_root: &Path,
) -> (
    tomet_semantics::DocumentConfig,
    tomet_compute::EvaluationContext,
) {
    let mut config = tomet_semantics::document_config(doc);
    if let Some((vault, _, _)) = tomet_config::find_config_file(file_path) {
        for (name, template) in vault.macros {
            config.macros.entry(name).or_insert(template);
        }
    }

    let rel = file_path
        .strip_prefix(project_root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .replace('\\', "/");
    let rel = rel.strip_prefix("./").unwrap_or(&rel).to_string();
    let filename = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let vars = tomet_compute::EvaluationContext::new().with_var(
        "self",
        tomet_ast::Value::Map(vec![
            ("path".to_string(), tomet_ast::Value::String(rel)),
            ("filename".to_string(), tomet_ast::Value::String(filename)),
        ]),
    );
    (config, vars)
}
