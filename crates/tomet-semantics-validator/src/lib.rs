mod error;
mod id;

pub use error::ValidationError;

use id::collect_ids;
use tomet_ast::Document;

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
        // args+value merge (`tomet-doc-walker`'s `element_attrs_view`)
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
}
