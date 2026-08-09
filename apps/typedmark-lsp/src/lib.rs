//! Diagnostics-only v1 of the TypedMark language server: parses a `.tm`
//! buffer with `typedmark-parser` on every open/change and republishes
//! whatever parse error comes back (or clears diagnostics if it parses
//! clean). No hover/completion/goto-definition yet -- see
//! `.agents/tasks/lsp-diagnostics.md` (or its history, once deleted) for
//! the follow-up scope.

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, TextEdit};

/// Parse `text` and turn the result into the diagnostics list for one
/// document. `typedmark-parser` reports at most one error per parse (it
/// stops at the first failure), so this is 0 or 1 items today; the
/// return type is a `Vec` so a future multi-error parser doesn't need a
/// signature change here.
pub fn diagnostics_for(text: &str) -> Vec<Diagnostic> {
    match typedmark_parser::parse_document(text) {
        Ok(_) => Vec::new(),
        Err(err) => vec![Diagnostic {
            range: error_range(&err, text),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("typedmark".to_string()),
            message: err.message.clone(),
            ..Diagnostic::default()
        }],
    }
}

/// `typedmark_parser::Error::{line,column}` are 1-based and counted in
/// `char`s (see `typedmark-lexar`'s `Cursor::line_col`), not the UTF-16
/// code units LSP `Position` wants. Treating the column as if it were
/// already a UTF-16 offset is only wrong for astral-plane characters
/// (surrogate pairs), which don't show up in realistic `.tm` prose --
/// a real converter is a known gap, not implemented here (v1 approx).
fn error_range(err: &typedmark_parser::Error, text: &str) -> Range {
    let line = (err.line.saturating_sub(1)) as u32;
    let col = (err.column.saturating_sub(1)) as u32;
    let start = Position::new(line, col);
    // Highlight through end-of-line rather than a zero-width point, so
    // the squiggle is actually visible in an editor.
    let line_len = text
        .lines()
        .nth(line as usize)
        .map(|l| l.chars().count() as u32)
        .unwrap_or(col);
    let end = Position::new(line, line_len.max(col));
    Range::new(start, end)
}

/// `textDocument/formatting` result for one document: a single edit
/// replacing the whole document with `typedmark_formatter::format_source`'s
/// output, or no edits at all if it's already formatted (some clients
/// apply an empty edit list as a visible no-op undo entry, so avoid that).
pub fn format_edits(text: &str) -> Vec<TextEdit> {
    let formatted = typedmark_formatter::format_source(text);
    if formatted == text {
        return Vec::new();
    }
    vec![TextEdit {
        range: whole_document_range(text),
        new_text: formatted,
    }]
}

/// Same char-counting approximation as `error_range` above (see its doc
/// comment for the UTF-16 caveat): a range from the start of the document
/// to just past its last character, used to replace the entire buffer.
fn whole_document_range(text: &str) -> Range {
    let lines: Vec<&str> = text.split('\n').collect();
    let end_line = lines.len().saturating_sub(1) as u32;
    let end_char = lines.last().map(|l| l.chars().count()).unwrap_or(0) as u32;
    Range::new(Position::new(0, 0), Position::new(end_line, end_char))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_document_has_no_diagnostics() {
        assert_eq!(diagnostics_for("#[ Hello ]\n"), Vec::new());
    }

    #[test]
    fn invalid_document_reports_one_diagnostic() {
        let diags = diagnostics_for("<caution>[ unterminated\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diags[0].source.as_deref(), Some("typedmark"));
    }

    #[test]
    fn already_formatted_document_has_no_edits() {
        assert_eq!(format_edits("#[ Hello ]\n"), Vec::new());
    }

    #[test]
    fn messy_document_gets_one_whole_document_edit() {
        let text = "#[ Hello ]  \n\n\n\n- one\n";
        let edits = format_edits(text);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "#[ Hello ]\n\n- one\n");
        assert_eq!(
            edits[0].range,
            Range::new(Position::new(0, 0), Position::new(5, 0))
        );
    }
}
