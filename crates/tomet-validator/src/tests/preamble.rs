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

fn deck_bindings(vocabulary: &str) -> Bindings {
    let vocab = tomet_semantics::Vocabulary::from_file(&parse(vocabulary)).expect("a vocabulary");
    Bindings::for_document(&parse("::kind(deck)\n"), [vocab])
}

const DECK: &str = "::kind(vocabulary)\n::vocabulary(deck)\n\n\
    ::attribute(character: (singleton: true, args: (name: (type: string, positional: true, required: true))), page: (host: ctmt))\n\
    ::element(card: ())\n";

#[test]
fn a_vocabulary_attribute_is_written_with_double_colon() {
    let bindings = deck_bindings(DECK);
    let doc = parse("::kind(deck)\n::character(alice)\n\ntext\n");
    let errors = validate_document_with(&doc, &bindings);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn a_vocabulary_attribute_is_held_to_its_entry() {
    let bindings = deck_bindings(DECK);
    // its arguments
    let errors = validate_document_with(
        &parse("::kind(deck)\n::character(alice, nme: x)\n\ntext\n"),
        &bindings,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, Diagnostic::UnknownArgument { argument, .. } if argument == "nme")
        ),
        "{errors:?}"
    );
    // `singleton: true`
    let errors = validate_document_with(
        &parse("::kind(deck)\n::character(a)\n::character(b)\n\ntext\n"),
        &bindings,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, Diagnostic::DuplicateSingleton { name, .. } if name == "character")
        ),
        "{errors:?}"
    );
    // `host: ctmt` in a `.tmt`
    let errors = validate_document_with(&parse("::kind(deck)\n::page()\n\ntext\n"), &bindings);
    assert!(
        errors.iter().any(
            |e| matches!(e, Diagnostic::NotForThisHost { declaration, .. } if declaration == "page")
        ),
        "{errors:?}"
    );
}

#[test]
fn which_table_a_name_is_in_decides_its_sigil() {
    let bindings = deck_bindings(DECK);
    let errors = validate_document_with(
        &parse("::kind(deck)\n::card()\n\n@character(alice)\n"),
        &bindings,
    );
    assert!(
        errors.iter().any(|e| matches!(e, Diagnostic::WrongSigil { name, written: "::", expected: "@", .. } if name == "card")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| matches!(e, Diagnostic::WrongSigil { name, written: "@", expected: "::", .. } if name == "character")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.to_string() == "`character` is an attribute, about the document: write `::character`, not `@character`"),
        "{errors:?}"
    );
}

#[test]
fn an_undeclared_double_colon_name_is_an_unknown_attribute() {
    let errors = validate_document_with(
        &parse("::kind(deck)\n::upstream(a: 1)\n\ntext\n"),
        &deck_bindings(DECK),
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownAttribute { name, .. } if name == "upstream")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e
            .to_string()
            .starts_with("unknown attribute `upstream`: only `std`")),
        "{errors:?}"
    );
}

#[test]
fn an_element_and_an_attribute_may_share_a_name() {
    let bindings = deck_bindings(
        "::kind(vocabulary)\n::vocabulary(deck)\n::attribute(card: ())\n::element(card: ())\n",
    );
    let errors = validate_document_with(&parse("::kind(deck)\n::card()\n\n@card()\n"), &bindings);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn region_is_no_field_of_an_element() {
    let errors = validate_document(&parse(
        "::kind(vocabulary)\n::vocabulary(deck)\n\n::element(character: (region: preamble))\n",
    ));
    assert!(
        errors.iter().any(|e| e
            .to_string()
            .contains("unknown field `region`; an element has")),
        "{errors:?}"
    );
}

fn used_deck_bindings(document: &str) -> Bindings {
    let vocab = tomet_semantics::Vocabulary::from_file(&parse(DECK)).expect("a vocabulary");
    Bindings::for_document(&parse(document), [vocab])
}

/// A dotted `::` head is namespaced, as an element's is: the attribute of the vocabulary the
/// document writes that namespace for.
#[test]
fn a_dotted_attribute_is_the_attribute_of_its_namespace() {
    for src in [
        "::kind(deck)\n::deck.character(alice)\n\ntext\n",
        "::kind(note)\n::use(deck)\n::deck.character(alice)\n\ntext\n",
        "::kind(note)\n::use(deck, as: d)\n::d.character(alice)\n\ntext\n",
        "::std.kind(deck)\n::character(alice)\n\ntext\n",
    ] {
        let errors = validate_document_with(&parse(src), &used_deck_bindings(src));
        assert!(errors.is_empty(), "{src}: {errors:?}");
    }
    // under an alias, the namespace written out is not in scope
    let src = "::kind(note)\n::use(deck, as: d)\n::deck.character(alice)\n\ntext\n";
    let errors = validate_document_with(&parse(src), &used_deck_bindings(src));
    assert!(
        matches!(
            errors.as_slice(),
            [Diagnostic::UnknownAttribute { name, unbound_namespace: Some(ns), .. }]
                if name == "deck.character" && ns == "deck"
        ),
        "{errors:?}"
    );
    // a singleton is counted by what it is, however it is written
    let src = "::kind(deck)\n::character(a)\n::deck.character(b)\n\ntext\n";
    let errors = validate_document_with(&parse(src), &used_deck_bindings(src));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateSingleton { .. })),
        "{errors:?}"
    );
    // the arguments are its entry's
    let src = "::kind(note)\n::use(deck, as: d)\n::d.character(alice, nme: x)\n\ntext\n";
    let errors = validate_document_with(&parse(src), &used_deck_bindings(src));
    assert!(
        errors.iter().any(
            |e| matches!(e, Diagnostic::UnknownArgument { argument, .. } if argument == "nme")
        ),
        "{errors:?}"
    );
}

/// The host check says which files may write the attribute.
#[test]
fn an_attribute_out_of_its_host_says_where_it_may_be_written() {
    let src = "::kind(deck)\n::page()\n\ntext\n";
    let errors = validate_document_with(&parse(src), &used_deck_bindings(src));
    let message = errors
        .iter()
        .find(|e| matches!(e, Diagnostic::NotForThisHost { .. }))
        .map(ToString::to_string);
    assert_eq!(
        message.as_deref(),
        Some("`::page` may only be written in a `.ctmt` file")
    );
}

/// What a `::use` may not be: each is reported at the `::use`.
#[test]
fn a_use_is_held_to_its_rules() {
    for (src, want) in [
        ("::use(deck, from: x)\n", "`::use` has no argument `from`"),
        (
            "::use(deck, as: \"a b\")\n",
            "`as:` of `::use` takes a bare name",
        ),
        (
            "::use(deck, as: std)\n",
            "`as: std`: `std` is a namespace tomet carries itself",
        ),
        (
            "::use(deck, as: doc)\n",
            "`as: doc`: `doc` is a namespace tomet carries itself",
        ),
        (
            "::kind(note)\n::use(deck, as: note)\n",
            "`as: note`: `note` is this document's own `::kind`",
        ),
        (
            "::use(deck, as: cards)\n::use(cards)\n",
            "`as: cards`: `cards` already names the vocabulary `cards`",
        ),
        (
            "::use(deck)\n::use(deck, as: d)\n",
            "`deck` is brought in by an earlier `::use` already",
        ),
    ] {
        let doc = parse(&format!("{src}\ntext\n"));
        let errors = validate_document_with(&doc, &used_deck_bindings(src));
        let uses: Vec<String> = errors
            .iter()
            .filter(|e| matches!(e, Diagnostic::InvalidUse { .. }))
            .map(ToString::to_string)
            .collect();
        assert!(
            matches!(uses.as_slice(), [one] if one.starts_with(want)),
            "{src}: {uses:?}"
        );
    }
}
