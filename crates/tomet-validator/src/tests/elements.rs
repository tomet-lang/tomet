//! Elements: their arguments, their content, and what they are declared to take.

use super::*;

/// A positional argument fills the slot its `args:` entry declares, so it
/// is not reported as an unknown argument keyed `""`.
#[test]
fn a_positional_argument_lands_on_its_declared_slot() {
    let doc = parse("::kind(deck)\n\n@card(3)\n");
    let errors = validate_document_with(&doc, &deck_with_params());
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn an_undeclared_argument_is_reported() {
    let doc = parse("::kind(deck)\n\n@card(id: 3, colour: red)\n");
    let errors = validate_document_with(&doc, &deck_with_params());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::UnknownArgument { argument, .. } if argument == "colour"
        )),
        "{errors:?}"
    );
}

/// A vocabulary with `content: { allow: }` declared, for
/// `check_content_shape`'s own tests.
fn deck_with_content_allow() -> Bindings {
    let vocab = tomet_semantics::Vocabulary::from_file(&parse(
        r#"::kind(vocabulary)
::vocabulary(deck)

::element(
  list: (
    display: block
    // `entry` resolves bare here (the document's own `::kind(deck)`), which
    // `Bindings::classify` normalizes to `deck.entry` even though it's
    // written bare -- same pre-existing quirk `check_one_rule`'s own doc
    // comment notes for `::rule(allow:...)`, so the allow-list has to name
    // it the same way.
    content: (allow: (deck.entry))
    about: | カードの一覧。
  )
  entry: (
    display: block
    content: (allow: inline)
    about: | 一覧の一項目。
  )
)
"#,
    ))
    .expect("a vocabulary");
    Bindings::for_document(&parse("::kind(deck)\n"), [vocab])
}

#[test]
fn content_allowed_by_name_is_clean() {
    let doc = parse("::kind(deck)\n\n@list[\n  @entry[ one ]\n  @entry[ two ]\n]\n");
    let errors = validate_document_with(&doc, &deck_with_content_allow());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::DisallowedInContent { .. })),
        "{errors:?}"
    );
}

#[test]
fn content_not_in_the_allow_list_is_reported() {
    let doc = parse("::kind(deck)\n\n@list[\n  @em[ not an entry ]\n]\n");
    let errors = validate_document_with(&doc, &deck_with_content_allow());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::DisallowedInContent { name, element, .. }
                if name == "em" && element == "list"
        )),
        "{errors:?}"
    );
}

#[test]
fn content_declared_inline_only_rejects_multiple_blocks() {
    let doc = parse("::kind(deck)\n\n@entry[\n  one\n\n  two\n]\n");
    let errors = validate_document_with(&doc, &deck_with_content_allow());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::ContentNotInline { name, .. } if name == "entry"
        )),
        "{errors:?}"
    );
}

#[test]
fn a_builtin_inline_only_kind_rejects_multiple_blocks() {
    let doc = parse("@em[\n  one\n\n  two\n]\n");
    let errors = validate_document_with(&doc, &Bindings::default());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::ContentNotInline { name, .. } if name == "em"
        )),
        "{errors:?}"
    );
}

#[test]
fn a_builtin_block_permitting_kind_allows_multiple_paragraphs() {
    let doc = parse("@quote[\n  one\n\n  two\n]\n");
    let errors = validate_document_with(&doc, &Bindings::default());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::ContentNotInline { .. })),
        "{errors:?}"
    );
}

#[test]
fn a_missing_required_argument_is_reported() {
    let doc = parse("::kind(deck)\n\n@card(tags: a)\n");
    let errors = validate_document_with(&doc, &deck_with_params());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::MissingRequiredArgument { argument, .. } if argument == "id"
        )),
        "{errors:?}"
    );
}

/// The whole point of `Value::Element`/`doc.icon`: an element embedded
/// as another element's *argument value* (not its own `(args)`/
/// `[content]`) still gets classified and checked, because
/// `tomet_tree::for_each_element` descends into `el.args` too.
/// `validate_document` (no vault, `Bindings::default()`) is the exact
/// path that motivated `doc` resolving unconditionally in
/// `Bindings::classify`.
#[test]
fn an_element_embedded_in_another_elements_args_is_still_checked() {
    let doc = parse("::meta(icon: @doc.icon(pkg:\"lucide\"))\n");
    let errors = validate_document(&doc);
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::MissingRequiredArgument { argument, .. } if argument == "name"
        )),
        "{errors:?}"
    );
}

#[test]
fn a_valid_embedded_doc_icon_has_no_errors() {
    let doc = parse("::meta(icon: @doc.icon(\"triangle\", pkg:\"lucide\"))\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn an_unregistered_name_under_doc_is_still_unknown_when_embedded() {
    let doc = parse("::meta(icon: @doc.glyph(\"triangle\"))\n");
    let errors = validate_document(&doc);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownElement { name, .. } if name == "doc.glyph")),
        "{errors:?}"
    );
}

/// An element whose declaration has no `@args` says nothing about its
/// arguments. Saying it takes none is a different statement, and one
/// the vocabulary has no spelling for yet.
#[test]
fn an_element_with_no_args_declaration_is_not_checked() {
    let doc = parse("::kind(deck)\n\n@plain(anything: 1)\n");
    let errors = validate_document_with(&doc, &deck_with_params());
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownArgument { .. })),
        "{errors:?}"
    );
}

/// `@draft` and `@fixme` are warnings, and carry the note so the
/// report says what is missing rather than only where.
#[test]
fn draft_and_fixme_are_warnings_that_carry_their_note() {
    let doc = parse(
        "::kind(note)\n\n@draft[ ローカル保存の理由 ]\n\n地の文に @fixme[ 直す ] も置ける。\n",
    );
    let diagnostics = validate_document(&doc);

    let draft = diagnostics
        .iter()
        .find(|d| matches!(d, Diagnostic::Draft { .. }))
        .unwrap_or_else(|| panic!("{diagnostics:?}"));
    assert_eq!(draft.severity(), Severity::Warning);
    assert!(draft.to_string().contains("ローカル保存の理由"), "{draft}");

    let fixme = diagnostics
        .iter()
        .find(|d| matches!(d, Diagnostic::Fixme { .. }))
        .unwrap_or_else(|| panic!("{diagnostics:?}"));
    assert_eq!(fixme.severity(), Severity::Warning);
}
