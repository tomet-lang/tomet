use lsp_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::position::TextPositions;

/// Parses `text` and produces diagnostics (both parse errors and CST validation rules).
pub fn diagnostics_for(text: &str) -> Vec<Diagnostic> {
    let positions = TextPositions::new(text);
    let mut diags = Vec::new();
    let cst = tomet_parser::parse_cst(text);

    // 1. CST-based lint validation (exact token wave lines!)
    let cst_errors = tomet_validator::validate_cst(&cst);
    for err in cst_errors {
        diags.push(Diagnostic {
            range: positions.text_range(err.range()),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("tomet".to_string()),
            message: err.to_string(),
            ..Diagnostic::default()
        });
    }

    // 2. High-level parser errors if document structure is invalid
    if let Err(err) = tomet_parser::parse_document(text) {
        diags.push(Diagnostic {
            range: error_range(&err, &positions),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("tomet".to_string()),
            message: err.message.clone(),
            ..Diagnostic::default()
        });
    }

    diags
}

fn error_range(err: &tomet_parser::Error, positions: &TextPositions<'_>) -> Range {
    positions.offset_to_line_end(err.offset)
}
