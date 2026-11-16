//! The preamble: the `::` declarations that open a document.

use super::*;

/// Several `::use` in the preamble is fine -- you bring in several
/// vocabularies. That `use` is preamble-only and not a singleton is
/// what proves the two axes are independent.
#[test]
fn several_preamble_elements_are_fine_when_they_are_not_singletons() {
    let doc = parse("::kind(note)\n::use(deck)\n::use(cards)\n\n#[ T ]\n");
    let errors = validate_document(&doc);
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateSingleton { .. })),
        "{errors:?}"
    );
}

/// A `::name` declaration is a preamble element by its sigil: no vocabulary is
/// read to know it, so even a name nothing declares is held to the preamble.
#[test]
fn a_double_colon_declaration_after_the_body_is_reported() {
    for src in [
        "::kind(note)\n\n#[ T ]\n\n::use(deck)\n",
        "::kind(note)\n\n=[ T ]\n\n::use(deck)\n",
        "::kind(note)\n\ntext\n\n::nonesuch()\n",
    ] {
        assert!(tomet_ast_parser::parse_file(src).is_err(), "{src:?}");
    }
}

#[test]
fn a_run_of_double_colon_declarations_at_the_top_is_the_preamble() {
    let doc = parse("::kind(note)\n::use(deck)\n::use(cards)\n::meta(id: x)\n\n=[ T ]\n");
    let errors = validate_document(&doc);
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            Diagnostic::DuplicateSingleton { .. } | Diagnostic::WrongSigil { .. }
        )),
        "{errors:?}"
    );
}

#[test]
fn the_two_spellings_share_one_preamble_and_one_singleton_count() {
    let errors = validate_document(&parse("::kind(note)\n::kind(note)\n"));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateSingleton { name, .. } if name == "kind")),
        "{errors:?}"
    );
    let errors = validate_document(&parse("::kind(note)\n::meta(title: x)\n\n=[ T ]\n"));
    assert!(errors.is_empty(), "{errors:?}");
}

/// The sigil says "this describes the document", so it is wrong on an element
/// that is content.
#[test]
fn a_body_element_written_with_double_colon_is_reported_with_the_sigil_to_use() {
    let errors = validate_document(&parse("::callout(note)[ hi ]\n"));
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::WrongSigil { name, written: "::", expected: "@", .. } if name == "callout"
        )),
        "{errors:?}"
    );
}

#[test]
fn a_user_declared_preamble_element_is_a_declaration() {
    let vocab = tomet_semantics::Vocabulary::from_file(&parse(
        "::kind(vocabulary)\n::vocabulary(deck)\n\n::element(character: (region: preamble, singleton: true))\n",
    ))
    .expect("a vocabulary");
    let bindings = Bindings::for_document(&parse("::kind(deck)\n"), [vocab]);
    let doc = parse("::kind(deck)\n::character(a: 1)\n\ntext\n");
    let errors = validate_document_with(&doc, &bindings);
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::WrongSigil { .. })),
        "{errors:?}"
    );
}
