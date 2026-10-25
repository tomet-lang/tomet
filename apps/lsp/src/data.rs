//! A standalone cotomet file (`.ctmt`) as the language server serves it.
//!
//! It is data, not a document: the cotomet parser says what is wrong with it,
//! the whitespace formatter formats it, and the features that read a document
//! (hover, symbols, completion, folding, definition) have nothing to say. It
//! never reaches the vault, which holds documents.

use lsp_types::{Diagnostic, DiagnosticSeverity, Uri};
use tomet_load::is_error;

use crate::position::TextPositions;

/// Whether `uri` names a `.ctmt` file. The query and fragment are not part of
/// the name.
pub fn is_data_uri(uri: &Uri) -> bool {
    const EXTENSION: &[u8] = b".ctmt";
    let path = uri.path().as_str().as_bytes();
    path.len() > EXTENSION.len()
        && path[path.len() - EXTENSION.len()..].eq_ignore_ascii_case(EXTENSION)
}

/// Whether a request's `params` are about a `.ctmt`: the document a request
/// names is at `textDocument.uri` in all of them.
pub fn request_is_for_data(params: &serde_json::Value) -> bool {
    params
        .pointer("/textDocument/uri")
        .and_then(serde_json::Value::as_str)
        .and_then(|uri| uri.parse::<Uri>().ok())
        .is_some_and(|uri| is_data_uri(&uri))
}

/// What the cotomet parser and evaluator say of `text` as a standalone file.
///
/// Returns parse errors, declaration errors, and expression evaluation errors,
/// underlined at their locations in the source text.
pub fn data_diagnostics_for(text: &str) -> Vec<Diagnostic> {
    let mut hook = tomet_parser::CotometValueHook;
    let positions = TextPositions::new(text);
    let parsed = match cotomet::syntax::parse_file_with(text, &mut hook) {
        Ok(parsed) => parsed,
        Err(err) => {
            return vec![Diagnostic {
                range: positions.offset_to_line_end(err.offset),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("tomet".to_string()),
                message: err.message,
                ..Diagnostic::default()
            }];
        }
    };

    let positions = TextPositions::new(text);
    let header_spans: Vec<_> = parsed.header.iter().map(|d| d.span).collect();

    let mut out = Vec::new();

    let declared: Vec<_> = parsed
        .header
        .iter()
        .map(|d| tomet_validator::Declared::new(tomet_declaration::Declaration::from(d), d.span))
        .collect();
    let mut decl_diagnostics = Vec::new();
    tomet_validator::check_ctmt_declarations(&declared, &mut decl_diagnostics);

    for d in &decl_diagnostics {
        out.push(Diagnostic {
            range: positions.range(&d.span()),
            severity: Some(if is_error(d) {
                DiagnosticSeverity::ERROR
            } else {
                DiagnosticSeverity::WARNING
            }),
            source: Some("tomet".to_string()),
            message: d.message_in(positions.index()),
            ..Diagnostic::default()
        });
    }

    if let Err(errors) = cotomet_eval::eval_file(parsed, text, &[]) {
        for err in errors {
            if header_spans
                .iter()
                .any(|s| s.contains_offset(err.span.start))
            {
                continue;
            }
            let range = if err.span.is_dummy() || err.span.start == err.span.end {
                positions.offset_to_line_end(err.span.start)
            } else {
                positions.range(&err.span)
            };
            out.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("tomet".to_string()),
                message: err.message,
                ..Diagnostic::default()
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_expression_produces_no_diagnostics() {
        let text = "::macros{ vocab: \"x/${1}\" }\npath: ${macro.vocab(\"writ\")}\n";
        let diags = data_diagnostics_for(text);
        assert!(diags.is_empty());
    }

    #[test]
    fn invalid_macro_produces_diagnostic() {
        let text = "path: ${macro.nosuch(1)}\n";
        let diags = data_diagnostics_for(text);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "unknown macro `macro.nosuch`");
        assert_eq!(diags[0].range.start.line, 0);
        assert_eq!(diags[0].range.start.character, 6);
    }

    #[test]
    fn declaration_error_produces_diagnostic() {
        let text = "::use(deck)\nkey: 1\n";
        let diags = data_diagnostics_for(text);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0]
                .message
                .contains("`::use` is not permitted in this host")
        );
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn bad_vars_entry_produces_one_diagnostic() {
        let text = "::vars{ bad: 123 }\nkey: 1\n";
        let diags = data_diagnostics_for(text);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bad"));
    }
}
