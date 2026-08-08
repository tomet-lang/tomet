//! Diagnostics-only v1 of the TypedMark language server: parses a `.tm`
//! buffer with `typedmark-parser` on every open/change and republishes
//! whatever parse error comes back (or clears diagnostics if it parses
//! clean). No hover/completion/goto-definition yet -- see
//! `.agents/tasks/lsp-diagnostics.md` (or its history, once deleted) for
//! the follow-up scope.

use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

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
}
