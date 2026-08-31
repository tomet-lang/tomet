mod error;
mod id;

pub use error::{CstValidationError, ValidationError};

use id::{collect_ids, collect_ids_cst};
use tomet_ast::Document;
use tomet_cst::{SyntaxNode, TextRange};

/// Runs all validation rules against a parsed `Document` and returns every
/// violation found. Read-only: never mutates `doc`, never does I/O.
pub fn validate_document(doc: &Document) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut seen: Vec<(String, tomet_ast::Span)> = Vec::new();

    for (id, span) in collect_ids(doc) {
        if let Some((_, first)) = seen.iter().find(|(seen_id, _)| *seen_id == id) {
            errors.push(ValidationError::DuplicateId {
                id,
                first: *first,
                duplicate: span,
            });
        } else {
            seen.push((id, span));
        }
    }

    errors
}

/// Runs all validation rules directly against a Concrete Syntax Tree ([`SyntaxNode`])
/// and returns violations with exact byte [`TextRange`]s.
pub fn validate_cst(root: &SyntaxNode) -> Vec<CstValidationError> {
    let mut errors = Vec::new();
    let mut seen: Vec<(String, TextRange)> = Vec::new();

    for (id, range) in collect_ids_cst(root) {
        if let Some((_, first_range)) = seen.iter().find(|(seen_id, _)| *seen_id == id) {
            errors.push(CstValidationError::DuplicateId {
                id,
                first_range: *first_range,
                duplicate_range: range,
            });
        } else {
            seen.push((id, range));
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Document {
        tomet_parser::parse_document(source).expect("valid Tomet source")
    }

    #[test]
    fn no_ids_is_fine() {
        let doc = parse("plain paragraph, no ids here\n");
        assert_eq!(validate_document(&doc), vec![]);
    }

    #[test]
    fn unique_ids_is_fine() {
        let doc = parse("#[ one ]{id:a}\n#[ two ]{id:b}\n");
        assert_eq!(validate_document(&doc), vec![]);
    }

    #[test]
    fn duplicate_top_level_ids_are_reported() {
        let doc = parse("#[ one ]{id:a}\n#[ two ]{id:a}\n");
        let errors = validate_document(&doc);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ValidationError::DuplicateId { id, .. } if id == "a"
        ));
    }

    #[test]
    fn duplicate_id_between_heading_and_element_is_reported() {
        // A heading's `{id:...}` lives in `Element.value`, not
        // `Element.args` -- regression coverage for `Node::attrs()`'s
        // args+value merge (`ElementExt::attrs_view`)
        // making it visible here at all.
        let doc = parse("#[ one ]{id:a}\n\n<task>(id:a)\n");
        let errors = validate_document(&doc);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ValidationError::DuplicateId { id, .. } if id == "a"
        ));
    }

    #[test]
    fn duplicate_id_nested_inline_is_reported() {
        // The second `id:a` is on an element embedded inline inside a
        // paragraph's content, not a top-level block -- exercises the
        // `visit_inlines` recursion, not just top-level `Block`s.
        let doc = parse("#[ one ]{id:a}\n\ntext @(id:a) more text\n");
        let errors = validate_document(&doc);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ValidationError::DuplicateId { id, .. } if id == "a"
        ));
    }

    #[test]
    fn duplicate_id_with_integer_values_is_reported() {
        let doc = parse("#[ one ]{id: 42}\n#[ two ]{id: 42}\n");
        let errors = validate_document(&doc);
        assert_eq!(errors.len(), 1);
        assert!(matches!(
            &errors[0],
            ValidationError::DuplicateId { id, .. } if id == "42"
        ));
        assert_eq!(
            errors[0].to_string(),
            "duplicate id `42` (first defined at 1:1)"
        );
    }

    #[test]
    fn test_validate_cst_exact_range() {
        let src = "#[ one ]{id: duplicate}\n\n#[ two ]{id: duplicate}\n";
        let cst = tomet_parser::parse_cst(src);
        let errors = validate_cst(&cst);
        assert_eq!(errors.len(), 1);

        let err = &errors[0];
        assert_eq!(err.range().len(), tomet_cst::TextSize::from(9)); // "duplicate" has len 9
        let err_slice = &src[usize::from(err.range().start())..usize::from(err.range().end())];
        assert_eq!(err_slice, "duplicate"); // Exact token!
    }
}
