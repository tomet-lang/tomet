//! Ids: one id once, and one singleton once.

use super::*;

#[test]
fn no_ids_is_fine() {
    let doc = parse("plain paragraph, no ids here\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn unique_ids_is_fine() {
    let doc = parse("=[ one ]#(a)\n=[ two ]#(b)\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn duplicate_top_level_ids_are_reported() {
    let doc = parse("=[ one ]#(a)\n=[ two ]#(a)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateId { id, .. } if id == "a"
    ));
}

/// `singleton` was declared for six elements and enforced nowhere,
/// so two `::meta` -- or two `::kind`, which is two answers to what the
/// document is -- passed every check this project had.
#[test]
fn a_second_singleton_is_reported() {
    let doc = parse("::kind(note)\n::meta(a: 1)\n::meta(b: 2)\n\n#[ T ]\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateSingleton { name, .. } if name == "meta"
    ));
}

#[test]
fn a_repeated_standard_declaration_says_where_the_first_is() {
    let src = "::kind(note)\n::meta(a: 1)\n::meta(b: 2)\n\n#[ T ]\n";
    let doc = parse(src);
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let index = tomet_position::LineIndex::new(src);
    assert_eq!(
        errors[0].message_in(&index),
        "`meta` may appear once in a document; the first is at 2:1"
    );
}

/// A vocabulary's own `singleton: true` is enforced the same way. The
/// builtin table and a declaration are two sources for one rule, not
/// two rules.
#[test]
fn a_vocabularys_singleton_is_enforced_too() {
    let vocab = tomet_ast_parser::parse_file(
        "::kind(vocabulary)\n::vocabulary(deck)\n\n::element(spread: (singleton: true))\n",
    )
    .expect("vocabulary parses");
    let bindings = Bindings {
        kind: tomet_semantics::Vocabulary::from_file(&vocab),
        ..Bindings::default()
    };

    let doc = parse("::kind(deck)\n\n#[ T ]\n\n@spread()\n@spread()\n");
    let errors = validate_document_with(&doc, &bindings);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateSingleton { name, .. } if name == "spread"
    ));
}

#[test]
fn duplicate_id_between_heading_and_element_is_reported() {
    // `deck.task` is namespaced and `deck` is in scope, so the only
    // rule left to fire is the one being tested.
    let doc = parse("=[ one ]#(a)\n\n@deck.task()#(a)\n");
    let errors = validate_document_with(&doc, &with_deck());
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateId { id, .. } if id == "a"
    ));
}

#[test]
fn duplicate_id_nested_inline_is_reported() {
    // The second id is on an element embedded inline inside a
    // paragraph's content, not a top-level block -- exercises the
    // `visit_inlines` recursion, not just top-level `Block`s.
    let doc = parse("=[ one ]#(a)\n\ntext @deck.ref()#(a) more text\n");
    let errors = validate_document_with(&doc, &with_deck());
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateId { id, .. } if id == "a"
    ));
}

#[test]
fn duplicate_id_on_an_element_in_a_value_slot_is_reported() {
    // `@term[ A ]#(dup)` sits in a declaration's `{...}` as the value of a key. It is an anchor like
    // any other (`@link(#dup)` resolves), so a second `#(dup)` is a duplicate.
    let doc = parse("::blueprint(note){ a: @term[ A ]#(dup) }\n\n=[ H ]#(dup)\n");
    let errors = validate_document(&doc);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateId { id, .. } if id == "dup")),
        "{errors:?}"
    );
}

#[test]
fn a_term_of_a_dictionary_body_is_an_anchor_like_any_other() {
    let doc = parse("::dictionary(d)\n\n;@term(\"a\")[ A ]#(dup)\n\n=[ H ]#(dup)\n");
    let errors = validate_document(&doc);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateId { id, .. } if id == "dup")),
        "{errors:?}"
    );
}

#[test]
fn duplicate_id_in_section_data_is_reported() {
    let doc = parse("=[ H ]{ key: @term[ A ]#(dup) }\n\n=[ Other ]#(dup)\n");
    let errors = validate_document(&doc);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateId { id, .. } if id == "dup")),
        "{errors:?}"
    );
}

#[test]
fn duplicate_id_with_integer_values_is_reported() {
    let src = "=[ one ]#(42)\n=[ two ]#(42)\n";
    let doc = parse(src);
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateId { id, .. } if id == "42"
    ));
    // `Display` has no source text, so it leaves the related location out;
    // a renderer that holds the source asks for it.
    assert_eq!(errors[0].to_string(), "duplicate id `42`");
    assert_eq!(
        errors[0].message_in(&tomet_position::LineIndex::new(src)),
        "duplicate id `42` (first defined at 1:1)"
    );
}

#[test]
fn test_validate_cst_exact_range() {
    let src = "=[ one ]#(duplicate)\n\n=[ two ]#(duplicate)\n";
    let cst = tomet_cst_parser::parse_cst(src);
    let errors = validate_cst(&cst);
    assert_eq!(errors.len(), 1);

    let err = &errors[0];
    assert_eq!(err.range().len(), tomet_cst::TextSize::from(9)); // "duplicate" has len 9
    let err_slice = &src[usize::from(err.range().start())..usize::from(err.range().end())];
    assert_eq!(err_slice, "duplicate"); // Exact token!
}

#[test]
fn validate_cst_counts_only_the_id_group_not_any_attribute_named_id() {
    // A literal `id:` key -- nested in a map, or `@link`'s own -- is
    // ordinary data, never this slot, so none of these count.
    let src = "@a(){id: x}\n@b(){meta: (id: x)}\n@link[t](id: x)\n";
    let cst = tomet_cst_parser::parse_cst(src);
    assert!(validate_cst(&cst).is_empty());

    let src = "@a()#(x)\ntext @b()#(x)\n";
    let cst = tomet_cst_parser::parse_cst(src);
    assert_eq!(validate_cst(&cst).len(), 1);
}

#[test]
fn validate_cst_catches_a_duplicate_id_nested_inside_content() {
    // `[content]` holds blocks, so a section nested in it has a heading
    // whose `#(id)` is an `ID_GROUP` of the tree like any other, and a
    // duplicate of an id outside it is reported. That shape is the one
    // pinned here: a plain `@`-element's `#(id)` nested the same way would
    // be caught either way. This is the LSP's only duplicate-id diagnostic
    // (`apps/lsp` never calls the AST-based `validate_document`).
    let src = "@a()#(x)\n@outer[\n=[ Heading ]#(x)\n]\n";
    let cst = tomet_cst_parser::parse_cst(src);
    assert_eq!(validate_cst(&cst).len(), 1);
}

#[test]
fn valid_anchor_link_is_fine() {
    let doc = parse(
        "=[ Hello ]#(greeting)\n\n@link(#greeting)\n@link(id:greeting)\n@link[Click](#greeting)\n",
    );
    assert_eq!(validate_document(&doc), vec![]);
}
