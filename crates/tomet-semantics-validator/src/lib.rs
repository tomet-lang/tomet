pub mod blueprint;
mod error;
mod id;

pub use blueprint::*;
pub use error::{CstValidationError, ValidationError};

use id::{collect_ids, collect_ids_cst};
use tomet_ast::Document;
use tomet_cst::{SyntaxNode, TextRange};
use tomet_semantics::{Bindings, Shape};

/// Classifies `el` against `std` and the namespaces in scope.
///
/// `Sigil::Bare` and `Sigil::Dollar` carry no name and are not a
/// vocabulary question, so they go straight through.
fn classify_in(
    el: &tomet_ast::Element,
    bindings: &Bindings,
) -> Result<tomet_semantics::ElementKind, tomet_semantics::UnknownName> {
    match &el.sigil {
        tomet_ast::Sigil::Named(name) => bindings.classify(name),
        _ => tomet_semantics::classify(el),
    }
}

fn shape_str(shape: Shape) -> &'static str {
    match shape {
        Shape::Block => "a block element",
        Shape::Inline => "an inline element",
    }
}

/// Runs all validation rules against a parsed `Document` and returns every
/// violation found. Read-only: never mutates `doc`, never does I/O.
///
/// Knows only `std`, so every element from a vocabulary is reported as
/// unknown. Callers that can read the vault's vocabularies -- which means
/// callers that may do I/O -- should use [`validate_document_with`] and
/// pass what is in scope.
pub fn validate_document(doc: &Document) -> Vec<ValidationError> {
    validate_document_with(doc, &Bindings::default())
}

/// [`validate_document`], with the namespaces the document has in scope.
///
/// Still does no I/O: `bindings` arrives already loaded, by whoever was
/// allowed to read the files. That split is why this can consult a
/// vocabulary without the layer below it gaining the ability to open one.
pub fn validate_document_with(doc: &Document, bindings: &Bindings) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut seen: Vec<(String, tomet_ast::Span)> = Vec::new();

    // The parser deliberately accepts any well-formed name -- deciding
    // which names exist is a vocabulary question, and the parser is barred
    // from consulting one. So this is where an unknown name, or an
    // element written with the wrong shape, is reported.
    tomet_tree::for_each_element(doc, |el| {
        if let Err(unknown) = classify_in(el, bindings) {
            errors.push(ValidationError::UnknownElement {
                name: unknown.name,
                span: el.span,
            });
            return;
        }
        if let Some((found, expected)) = tomet_semantics::shape_mismatch(el) {
            errors.push(ValidationError::ShapeMismatch {
                name: el.sigil.name().map(|n| n.to_string()).unwrap_or_default(),
                found: shape_str(found),
                expected: shape_str(expected),
                span: el.span,
            });
        }
    });

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

    /// A document with `deck` in scope.
    ///
    /// These cases used to rely on a namespaced name passing
    /// unconditionally -- `classify_name` returned `Custom` for anything
    /// with a namespace, bound or not. That was the hole `Bindings`
    /// closed, so the namespace has to actually be in scope now, and
    /// saying so here is what keeps these tests about duplicate ids.
    fn with_deck() -> Bindings {
        let vocab = tomet_parser::parse_document(
            "@kind(vocabulary)\n@vocabulary(deck)\n\n@element(task){}[ A task. ]\n@element(ref){}[ A ref. ]\n",
        )
        .expect("vocabulary parses");
        Bindings {
            used: [(
                "deck".to_string(),
                tomet_semantics::Vocabulary::from_document(&vocab).expect("has a header"),
            )]
            .into_iter()
            .collect(),
            ..Bindings::default()
        }
    }

    #[test]
    fn duplicate_id_between_heading_and_element_is_reported() {
        // A heading's `{id:...}` lives in `Element.value`, not
        // `Element.args` -- regression coverage for `Node::attrs()`'s
        // args+value merge (`ElementExt::attrs_view`)
        // making it visible here at all.
        // `deck.task` is namespaced and `deck` is in scope, so the only
        // rule left to fire is the one being tested.
        let doc = parse("#[ one ]{id:a}\n\n@deck.task(id:a)\n");
        let errors = validate_document_with(&doc, &with_deck());
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
        let doc = parse("#[ one ]{id:a}\n\ntext @deck.ref(id:a) more text\n");
        let errors = validate_document_with(&doc, &with_deck());
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
