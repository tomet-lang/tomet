//! A standalone motem file (`.mtm`) as the language server serves it.
//!
//! It is data, not a document: the motem parser says what is wrong with it,
//! the whitespace formatter formats it, and the features that read a document
//! (hover, symbols, completion, folding, definition) have nothing to say. It
//! never reaches the vault, which holds documents.

use lsp_types::{Diagnostic, DiagnosticSeverity, Uri};

use crate::position::TextPositions;

/// Whether `uri` names a `.mtm` file. The query and fragment are not part of
/// the name.
pub fn is_data_uri(uri: &Uri) -> bool {
    let path = uri.path().as_str();
    path.len() > 4 && path[path.len() - 4..].eq_ignore_ascii_case(".mtm")
}

/// Whether a request's `params` are about a `.mtm`: the document a request
/// names is at `textDocument.uri` in all of them.
pub fn request_is_for_data(params: &serde_json::Value) -> bool {
    params
        .pointer("/textDocument/uri")
        .and_then(serde_json::Value::as_str)
        .and_then(|uri| uri.parse::<Uri>().ok())
        .is_some_and(|uri| is_data_uri(&uri))
}

/// What the motem parser says of `text` as a standalone file: at most one
/// error, underlined from where it is to the end of its line.
pub fn data_diagnostics_for(text: &str) -> Vec<Diagnostic> {
    let Err(err) = motem::syntax::parse_file(text) else {
        return Vec::new();
    };
    let positions = TextPositions::new(text);
    vec![Diagnostic {
        range: positions.offset_to_line_end(err.offset),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("tomet".to_string()),
        message: err.message,
        ..Diagnostic::default()
    }]
}
