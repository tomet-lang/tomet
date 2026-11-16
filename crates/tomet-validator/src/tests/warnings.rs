//! What is only a warning, and what is still an error.

use super::*;

/// `@conflict` is also a warning -- same reasoning as `@draft`/
/// `@fixme`: an unresolved conflict is a normal thing to commit and
/// share so someone else can resolve it, not a reason to fail the
/// run.
#[test]
fn conflict_is_a_warning() {
    let doc = parse("::kind(note)\n\n@conflict(a: [ ローカル ], b: [ リモート ])\n");
    let diagnostics = validate_document(&doc);

    let conflict = diagnostics
        .iter()
        .find(|d| matches!(d, Diagnostic::Conflict { .. }))
        .unwrap_or_else(|| panic!("{diagnostics:?}"));
    assert_eq!(conflict.severity(), Severity::Warning);
}

/// Both take either shape: a gap is sometimes a whole missing section
/// and sometimes a phrase inside a sentence, so neither placement is
/// a `ShapeMismatch`.
#[test]
fn an_unfinished_marker_is_not_a_shape_error_in_either_position() {
    let doc = parse("::kind(note)\n\n@draft[ 節まるごと ]\n\n文の途中の @draft[ 一語 ] も可。\n");
    assert!(
        !validate_document(&doc)
            .iter()
            .any(|d| matches!(d, Diagnostic::ShapeMismatch { .. })),
        "{:?}",
        validate_document(&doc)
    );
}

/// Everything else stays an error, so a green run means the document
/// is correct and not merely unfinished.
#[test]
fn an_unknown_element_is_still_an_error() {
    let doc = parse("::kind(note)\n\n@nonesuch()\n");
    let diagnostics = validate_document(&doc);
    assert!(diagnostics.iter().all(|d| d.severity() == Severity::Error));
}

/// The identity of a document is `::id(...)`; `id` in `::meta` is reported, not read.
#[test]
fn an_id_in_meta_is_reported_and_an_id_declaration_is_not() {
    let moved = validate_document(&parse("::meta(id: a, title: T)\n"));
    assert!(
        moved
            .iter()
            .any(|e| matches!(e, Diagnostic::MetaIdMoved { .. }))
    );

    let declared = validate_document(&parse("::id(a)\n::meta(title: T)\n"));
    assert!(declared.is_empty(), "{declared:?}");

    let twice = validate_document(&parse("::id(a)\n::id(b)\n"));
    assert!(
        twice
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateSingleton { name, .. } if name == "id"))
    );
}
