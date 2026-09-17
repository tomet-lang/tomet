use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::position::text_range_to_lsp_range;

/// Parses `text` and produces diagnostics (both parse errors and CST validation rules).
pub fn diagnostics_for(text: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let cst = tomet_parser::parse_cst(text);

    // 1. CST-based lint validation (exact token wave lines!)
    let cst_errors = tomet_validator::validate_cst(&cst);
    for err in cst_errors {
        diags.push(Diagnostic {
            range: text_range_to_lsp_range(text, err.range()),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("tomet".to_string()),
            message: err.to_string(),
            ..Diagnostic::default()
        });
    }

    // 2. High-level parser errors if document structure is invalid
    if let Err(err) = tomet_parser::parse_document(text) {
        diags.push(Diagnostic {
            range: error_range(&err, text),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("tomet".to_string()),
            message: err.message.clone(),
            ..Diagnostic::default()
        });
    }

    diags
}

fn error_range(err: &tomet_parser::Error, text: &str) -> Range {
    let line = (err.line.saturating_sub(1)) as u32;
    let col = (err.column.saturating_sub(1)) as u32;
    let start = Position::new(line, col);
    let line_len = text
        .lines()
        .nth(line as usize)
        .map(|l| l.chars().count() as u32)
        .unwrap_or(col);
    let end = Position::new(line, line_len.max(col));
    Range::new(start, end)
}
