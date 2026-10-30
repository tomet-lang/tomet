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
    let doc = parse("::kind(note)\n::meta{a: 1}\n::meta{b: 2}\n\n#[ T ]\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateSingleton { name, .. } if name == "meta"
    ));
}

#[test]
fn a_repeated_standard_declaration_says_where_the_first_is() {
    let src = "::kind(note)\n::meta{a: 1}\n::meta{b: 2}\n\n#[ T ]\n";
    let doc = parse(src);
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let index = tomet_lexer::LineIndex::new(src);
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
    let vocab = tomet_parser::parse_document(
        "::kind(vocabulary)\n::vocabulary(deck)\n\n::elements{ spread: { singleton: true } }\n",
    )
    .expect("vocabulary parses");
    let bindings = Bindings {
        kind: tomet_semantics::Vocabulary::from_document(&vocab),
        ..Bindings::default()
    };

    let doc = parse("::kind(deck)\n\n#[ T ]\n\n@spread{}\n@spread{}\n");
    let errors = validate_document_with(&doc, &bindings);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateSingleton { name, .. } if name == "spread"
    ));
}

/// These test `check_rule_connects` directly rather than through
/// `validate_document`, so the assertions stay about `:rule` alone --
/// nesting a block-shaped element (`@card`/`@heading`) inside another
/// element's `[content]` also trips the unrelated, pre-existing
/// `ShapeMismatch` check (inline position vs. block-required kind),
/// which is real but has nothing to do with what these tests check.
fn rule_errors(doc: &Document, bindings: &Bindings) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    check_rule_connects(doc, bindings, &mut errors);
    errors
}

#[test]
fn a_rule_with_only_allowed_descendants_is_clean() {
    let doc = parse(
        "@section[ @card(title:\"a\")[ x ] @card(title:\"b\")[ y ] ]:rule(allow:list(card))\n",
    );
    assert_eq!(rule_errors(&doc, &Bindings::default()), vec![]);
}

#[test]
fn a_disallowed_descendant_is_reported() {
    let doc = parse("@section[ @card(title:\"a\")[ x ] @heading[ y ] ]:rule(allow:list(card))\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "heading"
    ));
}

#[test]
fn direct_true_does_not_recurse_past_the_first_level() {
    let doc = parse("@section[ @card[ @heading[ z ] ] ]:rule(allow:list(card), direct:true)\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(
        errors,
        vec![],
        "direct:true should not see the nested heading"
    );
}

#[test]
fn without_direct_the_same_nested_heading_is_reported() {
    let doc = parse("@section[ @card[ @heading[ z ] ] ]:rule(allow:list(card))\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "heading"
    ));
}

#[test]
fn a_typoed_connect_name_is_reported() {
    let doc = parse("@section[ x ]:rulle(allow:list(card))\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::UnknownConnect { name, .. } if name == "rulle"
    ));
}

/// `allow:` can name a namespaced identifier (`ns.mycard`); checking
/// a descendant against it needs the document's resolved `Bindings`,
/// not just the parser's own view -- this is the scenario that
/// requires this check to live in the validator rather than earlier.
#[test]
fn allow_list_resolves_a_namespaced_custom_element_via_bindings() {
    let vocab = tomet_parser::parse_document(
        "::kind(vocabulary)\n::vocabulary(ns)\n\n::elements{ mycard: {} }\n",
    )
    .expect("vocabulary parses");
    let bindings = Bindings {
        used: std::collections::BTreeMap::from([(
            "ns".to_string(),
            tomet_semantics::Vocabulary::from_document(&vocab).expect("valid vocabulary"),
        )]),
        ..Bindings::default()
    };

    let doc = parse("@section[ @ns.mycard{} @card[a] ]:rule(allow:list(card, ns.mycard))\n");
    assert_eq!(rule_errors(&doc, &bindings), vec![]);
}

/// A heading's own `:rule(...)` (`Section.connects`, not an
/// `Element`'s) used to do nothing at all: `check_rule_connects` only
/// ever looked at `el.connects` via `for_each_element`, and a
/// `Section` is not an `Element`, so its connect was never checked
/// against anything underneath the heading.
#[test]
fn a_heading_rule_with_only_allowed_descendants_is_clean() {
    let doc = parse("=[ Notes ]:rule(allow:list(card))\n@card(title:\"a\")[ x ]\n");
    assert_eq!(rule_errors(&doc, &Bindings::default()), vec![]);
}

#[test]
fn a_disallowed_descendant_under_a_heading_rule_is_reported() {
    let doc = parse("=[ Notes ]:rule(allow:list(card))\n@heading[ not allowed ]\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "heading"
    ));
}

#[test]
fn a_typoed_connect_name_on_a_heading_is_reported() {
    let doc = parse("=[ Notes ]:rulle(allow:list(card))\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::UnknownConnect { name, .. } if name == "rulle"
    ));
}

/// The regression this whole group guards against: `rule` itself
/// must never be classified as if it were an ordinary document
/// element (`walk.rs`'s `walk_block` used to feed `Section.connects`
/// through the generic element walker, which is what let `classify_in`
/// -- a completely different check -- see `rule` and report it as an
/// unbound name). Goes through `validate_document`, not
/// `check_rule_connects` directly: that is the check this regresses,
/// and the one `check_rule_connects`'s own tests cannot see.
#[test]
fn a_heading_rule_connect_is_not_misclassified_as_an_unknown_element() {
    let doc = parse("=[ Notes ]:rule(allow:list(card))\n@card(title:\"a\")[ x ]\n");
    let errors = validate_document(&doc);
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownElement { name, .. } if name == "rule")),
        "{errors:?}"
    );
}

/// The preamble is the run of preamble-region elements at the top.
/// The first block that is not one of them ends it.
#[test]
fn a_preamble_element_after_the_body_is_reported() {
    let doc = parse("::kind(note)\n\n#[ T ]\n\n::use(deck)\n");
    let errors = validate_document(&doc);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::OutsidePreamble { name, .. } if name == "use")),
        "{errors:?}"
    );
}

/// Several `::use` in the preamble is fine -- you bring in several
/// vocabularies. That `use` is preamble-only and not a singleton is
/// what proves the two axes are independent.
#[test]
fn several_preamble_elements_are_fine_when_they_are_not_singletons() {
    let doc = parse("::kind(note)\n::use(deck)\n::use(cards)\n\n#[ T ]\n");
    let errors = validate_document(&doc);
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            Diagnostic::DuplicateSingleton { .. } | Diagnostic::OutsidePreamble { .. }
        )),
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
        "::kind(note)\n\ntext\n\n::nonesuch{}\n",
    ] {
        let errors = validate_document(&parse(src));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, Diagnostic::OutsidePreamble { .. })),
            "{src:?}: {errors:?}"
        );
    }
}

#[test]
fn a_run_of_double_colon_declarations_at_the_top_is_the_preamble() {
    let doc = parse("::kind(note)\n::use(deck)\n::use(cards)\n::meta{ id: x }\n\n=[ T ]\n");
    let errors = validate_document(&doc);
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            Diagnostic::DuplicateSingleton { .. }
                | Diagnostic::OutsidePreamble { .. }
                | Diagnostic::WrongSigil { .. }
        )),
        "{errors:?}"
    );
}

#[test]
fn the_two_spellings_share_one_preamble_and_one_singleton_count() {
    // transitional: the old spelling still reads, and counts as the same element
    let errors = validate_document(&parse("::kind(note)\n::kind(note)\n"));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateSingleton { name, .. } if name == "kind")),
        "{errors:?}"
    );
    let errors = validate_document(&parse("::kind(note)\n::meta{ id: x }\n\n=[ T ]\n"));
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::OutsidePreamble { .. })),
        "{errors:?}"
    );
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
    let vocab = tomet_semantics::Vocabulary::from_document(&parse(
        "::kind(vocabulary)\n::vocabulary(deck){}\n\n::elements{ character: { region: preamble, singleton: true } }\n",
    ))
    .expect("a vocabulary");
    let bindings = Bindings::for_document(&parse("::kind(deck)\n"), [vocab]);
    let doc = parse("::kind(deck)\n::character{ a: 1 }\n\ntext\n");
    let errors = validate_document_with(&doc, &bindings);
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            Diagnostic::WrongSigil { .. } | Diagnostic::OutsidePreamble { .. }
        )),
        "{errors:?}"
    );
}

/// A vocabulary with a declared parameter list.
fn deck_with_params() -> Bindings {
    let vocab = tomet_semantics::Vocabulary::from_document(&parse(
        r#"::kind(vocabulary)
::vocabulary(deck){}

::elements{
  card: {
    args: {
      id: {
        positional: true
        required: true
        about: | 通し番号。
      }
      tags: {
        about: | タグ。
      }
    }
    about: | カード。
  }
  plain: {
    about: | 宣言なし。
  }
}
"#,
    ))
    .expect("a vocabulary");
    Bindings::for_document(&parse("::kind(deck)\n"), [vocab])
}

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
    let vocab = tomet_semantics::Vocabulary::from_document(&parse(
        r#"::kind(vocabulary)
::vocabulary(deck){}

::elements{
  list: {
    display: block
    // `item` resolves bare here (the document's own `::kind(deck)`), which
    // `Bindings::classify` normalizes to `deck.item` even though it's
    // written bare -- same pre-existing quirk `check_one_rule`'s own doc
    // comment notes for `:rule(allow:...)`, so the allow-list has to name
    // it the same way.
    content: { allow: list(deck.item) }
    about: | カードの一覧。
  }
  item: {
    display: block
    content: { allow: inline }
    about: | 一覧の一項目。
  }
}
"#,
    ))
    .expect("a vocabulary");
    Bindings::for_document(&parse("::kind(deck)\n"), [vocab])
}

#[test]
fn content_allowed_by_name_is_clean() {
    let doc = parse("::kind(deck)\n\n@list[\n  @item[ one ]\n  @item[ two ]\n]\n");
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
    let doc = parse("::kind(deck)\n\n@list[\n  @em[ not an item ]\n]\n");
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
    let doc = parse("::kind(deck)\n\n@item[\n  one\n\n  two\n]\n");
    let errors = validate_document_with(&doc, &deck_with_content_allow());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::ContentNotInline { name, .. } if name == "item"
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
/// `tomet_tree::for_each_element` now descends into `el.args` too.
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
    let doc = parse("::kind(note)\n\n@nonesuch{}\n");
    let diagnostics = validate_document(&doc);
    assert!(diagnostics.iter().all(|d| d.severity() == Severity::Error));
}

/// The identity of a document is `::id(...)`; `id` in `::meta` is reported, not read.
#[test]
fn an_id_in_meta_is_reported_and_an_id_declaration_is_not() {
    let moved = validate_document(&parse("::meta{ id: a, title: T }\n"));
    assert!(
        moved
            .iter()
            .any(|e| matches!(e, Diagnostic::MetaIdMoved { .. }))
    );

    let declared = validate_document(&parse("::id(a)\n::meta{ title: T }\n"));
    assert!(declared.is_empty(), "{declared:?}");

    let twice = validate_document(&parse("::id(a)\n::id(b)\n"));
    assert!(
        twice
            .iter()
            .any(|e| matches!(e, Diagnostic::DuplicateSingleton { name, .. } if name == "id"))
    );
}

/// `elements:` described a custom element in `::settings`; a
/// `::vocabulary` document says all of it now, so the settings copy is
/// reported rather than skipped -- skipping is how it survived being
/// read by nothing.
#[test]
fn a_retired_settings_key_is_reported() {
    let doc = parse(
        "::kind(settings)\n::settings{\n\
         elements: { bookmark: { singleton: false } }\n}\n",
    );
    let errors = validate_document(&doc);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::RetiredSettingsKey { key, .. } if key == "elements")),
        "{errors:?}"
    );
}

/// `types:` goes with it, and its message has to say there is nowhere
/// to move it to rather than inventing a destination.
#[test]
fn the_types_map_is_retired_too_and_says_it_has_no_replacement() {
    let doc = parse(
        "::kind(settings)\n::settings{\n\
         types: { bookmark: { style: one_line } }\n}\n",
    );
    let errors = validate_document(&doc);
    let reported = errors
        .iter()
        .find(|e| matches!(e, Diagnostic::RetiredSettingsKey { key, .. } if key == "types"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert!(
        reported.to_string().contains("no replacement"),
        "{reported}"
    );
}

/// A macro is a definition of a name, not a setting: `macros:` in
/// `::settings` or `::config` is reported, and says where it goes.
#[test]
fn macros_in_the_settings_are_retired_and_say_where_they_go() {
    for head in ["settings", "config"] {
        let doc = parse(&format!(
            "::kind({head})\n::{head}{{\nmacros: {{ gh: \"https://example.com/${{1}}\" }}\n}}\n"
        ));
        let errors = validate_document(&doc);
        let reported = errors
            .iter()
            .find(|e| matches!(e, Diagnostic::RetiredSettingsKey { key, .. } if key == "macros"))
            .unwrap_or_else(|| panic!("{head}: {errors:?}"));
        assert!(reported.to_string().contains("::macros"), "{reported}");
    }
}

fn definition_problems(src: &str) -> Vec<String> {
    validate_document(&parse(src))
        .iter()
        .filter(|e| matches!(e, Diagnostic::InvalidDefinition { .. }))
        .map(|e| e.to_string())
        .collect()
}

/// A macro is a string template: a map or a number has nothing to substitute into.
#[test]
fn a_macro_that_is_not_a_string_is_reported() {
    assert!(definition_problems("::macros{\n  gh: \"https://x/${1}\"\n}\n").is_empty());
    let found = definition_problems("::macros{\n  gh: 3\n  ok: \"x\"\n}\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`gh` in `::macros`"), "{found:?}");
}

#[test]
fn a_variable_is_a_declaration_with_a_type_it_can_have_and_a_default_that_fits() {
    let good = "::vars{\n  title: {\n    type: string\n    about: | 題。\n  }\n  n: { type: int, default: 3 }\n  at: { type: datetime, default: \"2026-10-25T09:00:00+09:00\" }\n  tags: { type: list, default: list(a, b) }\n}\n";
    assert!(
        definition_problems(good).is_empty(),
        "{:?}",
        definition_problems(good)
    );

    for (src, needle) in [
        ("::vars{\n  title: \"x\"\n}\n", "declared as a map"),
        (
            "::vars{\n  title: { kind: string }\n}\n",
            "unknown key `kind`",
        ),
        ("::vars{\n  title: { type: date }\n}\n", "is not a type"),
        (
            "::vars{\n  n: { type: int, default: \"3\" }\n}\n",
            "`default` is not a value of `type: int`",
        ),
        (
            "::vars{\n  at: { type: datetime, default: \"tomorrow\" }\n}\n",
            "`default` is not a value of `type: datetime`",
        ),
    ] {
        let found = definition_problems(src);
        assert_eq!(found.len(), 1, "{src}: {found:?}");
        assert!(found[0].contains(needle), "{src}: {found:?}");
    }
}

/// The live surface is untouched. `format` and the path lists are what
/// `tomet-config` actually reads.
#[test]
fn a_settings_document_using_only_live_keys_is_clean() {
    let doc = parse(
        "::kind(config)\n::config{\n\
         format: { callout: { style: { content: block } } }\n\
         blueprints: list(\"a.blueprint.tmt\")\n}\n",
    );
    let errors = validate_document(&doc);
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::RetiredSettingsKey { .. })),
        "{errors:?}"
    );
}

/// A document with `deck` in scope.
///
/// These cases used to rely on a namespaced name passing
/// unconditionally -- `classify_std_name` returned `Custom` for anything
/// with a namespace, bound or not. That was the hole `Bindings`
/// closed, so the namespace has to actually be in scope now, and
/// saying so here is what keeps these tests about duplicate ids.
fn with_deck() -> Bindings {
    let vocab = tomet_parser::parse_document(
        "::kind(vocabulary)\n::vocabulary(deck)\n\n::elements{ task: {}, ref: {} }\n",
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
    // `deck.task` is namespaced and `deck` is in scope, so the only
    // rule left to fire is the one being tested.
    let doc = parse("=[ one ]#(a)\n\n@deck.task#(a)\n");
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
    let doc = parse("=[ one ]#(a)\n\ntext @deck.ref#(a) more text\n");
    let errors = validate_document_with(&doc, &with_deck());
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateId { id, .. } if id == "a"
    ));
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
        errors[0].message_in(&tomet_lexer::LineIndex::new(src)),
        "duplicate id `42` (first defined at 1:1)"
    );
}

#[test]
fn test_validate_cst_exact_range() {
    let src = "=[ one ]#(duplicate)\n\n=[ two ]#(duplicate)\n";
    let cst = tomet_parser::parse_cst(src);
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
    // ordinary data now, never this slot, so none of these count.
    let src = "@a{id: x}\n@b{meta: {id: x}}\n@link[t](id: x)\n";
    let cst = tomet_parser::parse_cst(src);
    assert!(validate_cst(&cst).is_empty());

    let src = "@a#(x)\ntext @b#(x)\n";
    let cst = tomet_parser::parse_cst(src);
    assert_eq!(validate_cst(&cst).len(), 1);
}

#[test]
fn validate_cst_catches_a_duplicate_id_nested_inside_content() {
    // `cst.rs::parse_content_group` used to parse `[content]` as flat
    // inline text only (`try_parse_inline_element`'s `@`/`^`/`$` sigils
    // only), never dispatching to block constructs like a section --
    // so a nested section heading's own `#(id)` was never wrapped in an
    // `ID_GROUP` at all; `=` isn't a sigil `try_parse_inline_element`
    // recognizes, so it stayed raw text. A plain `@`-element's own
    // `#(id)` nested the same way was already caught even before this
    // fix -- `try_parse_inline_element` reaches the same shared
    // `parse_groups`/`parse_id_group` regardless of bare-line status --
    // so that shape doesn't distinguish old from new behavior; a nested
    // section's heading-level `#(id)` does. This is the LSP's only
    // duplicate-id diagnostic (`apps/lsp` never calls the AST-based
    // `validate_document`).
    let src = "@a#(x)\n@outer[\n=[ Heading ]#(x)\n]\n";
    let cst = tomet_parser::parse_cst(src);
    assert_eq!(validate_cst(&cst).len(), 1);
}

#[test]
fn valid_anchor_link_is_fine() {
    let doc = parse(
        "=[ Hello ]#(greeting)\n\n@link(#greeting)\n@link(id:greeting)\n@link[Click](#greeting)\n",
    );
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn dangling_anchor_link_is_reported() {
    let doc = parse("@link(#missing)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor == "missing"
    ));
    assert_eq!(errors[0].severity(), Severity::Error);
    assert_eq!(errors[0].to_string(), "dangling anchor `#missing`");
}

#[test]
fn dangling_id_link_is_reported() {
    let doc = parse("@link(id:missing)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor == "missing"
    ));
}

#[test]
fn dangling_embed_is_reported() {
    let doc = parse("@embed(#missing)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor == "missing"
    ));
}

#[test]
fn valid_path_traversal_anchor_is_fine() {
    let doc = parse("@card#(intro)\n\n@link(#intro.prop.tags)\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn dangling_path_traversal_anchor_is_reported() {
    let doc = parse("@link(#missing.prop.tags)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor == "missing.prop.tags"
    ));
}

#[test]
fn positional_item_link_is_allowed() {
    let doc = parse("@link(#item(0))\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn external_and_doc_links_are_not_flagged_as_dangling_internal_anchors() {
    let doc = parse(
        "@link(https://example.com)\n@link(file:readme.md)\n@link(tmt:other.tmt#some)\n@link(ref:Note#some)\n",
    );
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn anchor_on_list_item_is_found() {
    let doc = parse("- item #(task1)\n\n@link(#task1)\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn empty_anchor_is_reported() {
    let doc = parse("@link(#)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DanglingAnchor { anchor, .. } if anchor.is_empty()
    ));
}

fn field_diagnostics(source: &str, declared: &[(&str, &str)]) -> Vec<Diagnostic> {
    let types = declared
        .iter()
        .map(|(field, field_type)| (field.to_string(), field_type.to_string()))
        .collect();
    let mut errors = Vec::new();
    check_field_types(&parse(source), &types, &mut errors);
    errors
}

const UUID: &str = "123e4567-e89b-42d3-a456-426614174000";

#[test]
fn a_uuid_field_holding_a_uuid_is_clean() {
    let source = format!("::id({UUID})\n");
    assert_eq!(field_diagnostics(&source, &[("id", "uuid")]), vec![]);
    let upper = format!("::id({})\n", UUID.to_uppercase());
    assert_eq!(field_diagnostics(&upper, &[("id", "uuid")]), vec![]);
}

#[test]
fn a_uuid_field_holding_anything_else_is_reported() {
    for bad in [
        "not-a-uuid",
        "123e4567e89b42d3a456426614174000",
        "123e4567-e89b-42d3-a456-42661417400",
        "123e4567-e89b-42d3-a456-42661417400g",
    ] {
        let source = format!("::id(\"{bad}\")\n");
        let errors = field_diagnostics(&source, &[("id", "uuid")]);
        assert_eq!(errors.len(), 1, "{bad}: {errors:?}");
        assert!(
            matches!(&errors[0], Diagnostic::InvalidFieldValue { field, value, .. }
                if field == "id" && value == bad),
            "{errors:?}"
        );
        assert_eq!(errors[0].severity(), Severity::Error);
    }
    let number = field_diagnostics("::id(42)\n", &[("id", "uuid")]);
    assert_eq!(number.len(), 1, "{number:?}");
}

/// What is not there to be wrong is not reported: a field the document
/// leaves out, one left empty for `tomet format` to generate, and an
/// expression that has not been evaluated yet (a blueprint's
/// `${generate.uuid()}`).
#[test]
fn an_absent_empty_or_unevaluated_uuid_is_not_reported() {
    let declared = [("id", "uuid")];
    assert_eq!(field_diagnostics("::meta{ title: x }\n", &declared), vec![]);
    assert_eq!(field_diagnostics("::id(\"\")\n", &declared), vec![]);
    assert_eq!(
        field_diagnostics("::id(${generate.uuid()})\n", &declared),
        vec![]
    );
    assert_eq!(field_diagnostics("plain\n", &declared), vec![]);
}

/// Only the fields the settings declare are looked at, and only the types
/// that have a form.
#[test]
fn an_undeclared_field_or_a_type_with_no_form_is_left_alone() {
    let source = "::id(nonsense)\n::meta{ title: nonsense }\n";
    assert_eq!(field_diagnostics(source, &[]), vec![]);
    assert_eq!(field_diagnostics(source, &[("title", "string")]), vec![]);
    assert_eq!(field_diagnostics(source, &[("id", "uuid")]).len(), 1);
}

#[test]
fn a_datetime_field_has_to_read_as_one() {
    let declared = [("created", "datetime")];
    for good in [
        "2026-10-17",
        "2026-10-17T09:00:00+09:00",
        "2026-10-17T09:00",
        "2026-10-17 09:00:00",
    ] {
        let source = format!("::meta{{ created: {good} }}\n");
        assert_eq!(field_diagnostics(&source, &declared), vec![], "{good}");
    }
    for bad in ["yesterday", "2026-13-01", "2026-10-17T25:00", "2026-1-1"] {
        let source = format!("::meta{{ created: \"{bad}\" }}\n");
        let errors = field_diagnostics(&source, &declared);
        assert_eq!(errors.len(), 1, "{bad}: {errors:?}");
        assert!(
            errors[0].to_string().contains("not a valid datetime"),
            "{}",
            errors[0]
        );
    }
    // Absent, empty and not yet evaluated are not reported, as for `uuid`.
    assert_eq!(field_diagnostics("::meta{ title: x }\n", &declared), vec![]);
    assert_eq!(
        field_diagnostics("::meta{ created: ${generate.now()} }\n", &declared),
        vec![]
    );
}

/// A document names only the major version, as an integer of 1 or more, and it
/// may leave the version out.
#[test]
fn a_version_after_the_name_of_kind_and_use_is_fine_when_it_is_an_integer() {
    for src in [
        "::kind(note)\n::use(deck)\n",
        "::kind(note, 1)\n::use(deck, 3)\n",
        "::kind(note, version: 2)\n",
    ] {
        let errors = validate_document(&parse(src));
        assert!(
            !errors.iter().any(|e| matches!(
                e,
                Diagnostic::InvalidVersion { .. } | Diagnostic::TooManyArguments { .. }
            )),
            "{src:?}: {errors:?}"
        );
    }
}

/// `1.0` is the semver habit this check exists for.
#[test]
fn a_version_that_is_not_an_integer_of_one_or_more_is_reported() {
    for (src, element, found) in [
        ("::kind(note, 1.0)\n", "kind", "1.0"),
        ("::kind(note, \"x\")\n", "kind", "\"x\""),
        ("::kind(note, 0)\n", "kind", "0"),
        ("::kind(note, -1)\n", "kind", "-1"),
        ("::kind(note)\n::use(deck, 1.0)\n", "use", "1.0"),
        ("::kind(note)\n::use(deck, 0)\n", "use", "0"),
    ] {
        let errors = validate_document(&parse(src));
        assert!(
            errors.iter().any(|e| matches!(
                e,
                Diagnostic::InvalidVersion { element: el, found: f, .. }
                    if el == element && f == found
            )),
            "{src:?}: {errors:?}"
        );
    }
}

#[test]
fn kind_and_use_take_a_name_and_a_version_and_nothing_more() {
    for (src, element) in [
        ("::kind(note, 1, 2)\n", "kind"),
        ("::kind(note)\n::use(deck, 1, 2)\n", "use"),
    ] {
        let errors = validate_document(&parse(src));
        assert!(
            errors.iter().any(|e| matches!(
                e,
                Diagnostic::TooManyArguments { element: el, takes: 2, .. } if el == element
            )),
            "{src:?}: {errors:?}"
        );
    }
}

#[test]
fn the_version_messages_say_what_to_write() {
    let errors = validate_document(&parse("::kind(note, 1.0)\n"));
    let message = errors
        .iter()
        .find(|e| matches!(e, Diagnostic::InvalidVersion { .. }))
        .expect("an invalid version is reported")
        .to_string();
    assert_eq!(
        message,
        "the version in `::kind(...)` must be an integer of 1 or more, found 1.0"
    );

    let errors = validate_document(&parse("::kind(note, 1, 2)\n"));
    let message = errors
        .iter()
        .find(|e| matches!(e, Diagnostic::TooManyArguments { .. }))
        .expect("an extra argument is reported")
        .to_string();
    assert_eq!(
        message,
        "`kind` takes at most 2 arguments: the name, then an optional version"
    );
}

fn version_problems(src: &str) -> Vec<String> {
    validate_document(&parse(src))
        .iter()
        .filter(|e| matches!(e, Diagnostic::InvalidDefinition { name, .. } if name == "version"))
        .map(|e| e.to_string())
        .collect()
}

/// A document's declared version is a version: `1.10` is the version `1.10`, bare or
/// quoted, and `abc` or `1.0-beta` is not one.
#[test]
fn a_declared_version_is_held_to_the_version_type() {
    for ok in [
        "::vocabulary(deck){ version: \"1.0.0\" }\n",
        "::vocabulary(deck){ version: 1.10 }\n",
        "::vocabulary(deck){ version: 2 }\n",
        "::blueprint(note){ version: \"1.0\", description: \"x\" }\n",
        "::version(1.0)\n",
        "::version(1.10)\n",
        "::version(1.10.0)\n",
    ] {
        assert!(
            version_problems(ok).is_empty(),
            "{ok}: {:?}",
            version_problems(ok)
        );
    }
    for (bad, needle) in [
        (
            "::vocabulary(deck){ version: \"abc\" }\n",
            "`abc` is not a version",
        ),
        (
            "::vocabulary(deck){ version: \"1.0-beta\" }\n",
            "`1.0-beta`",
        ),
        (
            "::blueprint(note){ version: true }\n",
            "`true` is not a version",
        ),
        ("::version(v1)\n", "`v1` is not a version"),
    ] {
        let found = version_problems(bad);
        assert_eq!(found.len(), 1, "{bad}: {found:?}");
        assert!(found[0].contains(needle), "{bad}: {found:?}");
    }
}

/// A `::meta` field declared `type: version` holds a bare `1.10` as the text it was
/// written as; the other forms it checks are as before.
#[test]
fn a_version_field_in_meta_reads_a_bare_number_as_text() {
    use std::collections::BTreeMap;
    let types: BTreeMap<String, String> = [("rev".to_string(), "version".to_string())]
        .into_iter()
        .collect();
    let problems = |src: &str| {
        let mut errors = Vec::new();
        check_field_types(&parse(src), &types, &mut errors);
        errors.len()
    };
    for ok in ["1.10", "\"1.10.0\"", "2", "1.0"] {
        assert_eq!(problems(&format!("::meta{{ rev: {ok} }}\n")), 0, "{ok}");
    }
    for bad in ["\"abc\"", "true", "\"1.0-beta\""] {
        assert_eq!(problems(&format!("::meta{{ rev: {bad} }}\n")), 1, "{bad}");
    }
}

const TYPED_DECK: &str = r#"::kind(vocabulary)
::vocabulary(deck){}

::elements{
  item: {
    args: {
      id: { type: uint, positional: true }
      label: { type: string }
      rev: { type: version }
      done: { type: bool }
      owner: { type: person }
      tags: { type: list(string) }
    }
  }
}
"#;

fn typed_deck() -> Bindings {
    let vocab =
        tomet_semantics::Vocabulary::from_document(&parse(TYPED_DECK)).expect("a vocabulary");
    Bindings::for_document(&parse("::kind(deck)\n"), [vocab])
}

fn argument_problems(src: &str) -> Vec<String> {
    validate_document_with(&parse(&format!("::kind(deck)\n\n{src}\n")), &typed_deck())
        .iter()
        .filter(|e| matches!(e, Diagnostic::InvalidArgument { .. }))
        .map(|e| e.to_string())
        .collect()
}

/// An argument is held to the type its parameter declares. A bare scalar of a text
/// type is the text it was written as, so `2024` is a `string` and `1.10` a `version`.
#[test]
fn an_argument_is_held_to_the_type_its_parameter_declares() {
    for ok in [
        "@item(3)",
        "@item(id: 0, label: 2024, rev: 1.10, done: true)",
        "@item(label: hello, rev: \"1.10.0\")",
        "@item(rev: 2)",
        "@item(label: true)",
        // not known until evaluated
        "@item(id: ${vars.n})",
        // `type: person` names no type and `list(string)` has no spelling yet: not checked
        "@item(owner: anyone, tags: list(a, b))",
        "@item(tags: 5)",
    ] {
        assert!(
            argument_problems(ok).is_empty(),
            "{ok}: {:?}",
            argument_problems(ok)
        );
    }
    for (bad, needle) in [
        ("@item(id: abc)", "`abc` is not a non-negative integer"),
        ("@item(id: -1)", "`-1` is not a non-negative integer"),
        ("@item(id: \"3\")", "`3` is not a non-negative integer"),
        ("@item(rev: 1.0-beta)", "`1.0-beta` is not a version"),
        ("@item(done: yes)", "`yes` is not `true` or `false`"),
        ("@item(label: list(a))", "a list is not a string"),
    ] {
        let found = argument_problems(bad);
        assert_eq!(found.len(), 1, "{bad}: {found:?}");
        assert!(found[0].contains(needle), "{bad}: {found:?}");
        assert!(found[0].contains("is declared `type:"), "{bad}: {found:?}");
    }
}

/// Checking the vocabulary document itself reports the parameters it declares in a
/// way that cannot be held to.
#[test]
fn checking_a_vocabulary_reports_a_type_that_names_no_type() {
    let errors = validate_document(&parse(TYPED_DECK));
    let problems: Vec<String> = errors
        .iter()
        .filter(|e| matches!(e, Diagnostic::InvalidDefinition { declaration, .. } if *declaration == "elements"))
        .map(|e| e.to_string())
        .collect();
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems
            .iter()
            .any(|p| p.contains("`item.owner`") && p.contains("`person` is not a type")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("`item.tags`") && p.contains("no spelling yet")),
        "{problems:?}"
    );
}

#[test]
fn check_ctmt_declarations_reports_expected_diagnostics() {
    use tomet_ast::{Span, Value};
    use tomet_declaration::Declaration;

    // 1. Valid declarations produce no errors
    let declared = vec![
        Declared::new(
            Declaration::new(
                "kind",
                Some(Value::Map(vec![
                    ("".into(), Value::String("catalog".into())),
                    ("".into(), Value::Int(1)),
                ])),
                None,
            ),
            Span::new(0, 10),
        ),
        Declared::new(
            Declaration::new("version", Some(Value::String("1.0.0".into())), None),
            Span::new(11, 25),
        ),
        Declared::new(
            Declaration::new(
                "meta",
                None,
                Some(Value::Map(vec![(
                    "author".into(),
                    Value::String("me".into()),
                )])),
            ),
            Span::new(26, 40),
        ),
    ];
    let mut errors = Vec::new();
    check_ctmt_declarations(&declared, &mut errors);
    assert_eq!(errors, vec![]);

    // 2. Non-standard declarations are passed over; NotForThisHost, Repeated, InvalidVersion, and InvalidDefinition are reported
    let decls = vec![
        Declaration::new("upstreams", None, None),
        Declaration::new("foo.bar", None, None),
        Declaration::new("use", Some(Value::String("deck".into())), None),
        Declaration::new("meta", None, None),
        Declaration::new("meta", None, None),
        Declaration::new(
            "kind",
            Some(Value::Map(vec![
                ("".into(), Value::String("catalog".into())),
                ("".into(), Value::String("abc".into())),
            ])),
            None,
        ),
        Declaration::new(
            "macros",
            None,
            Some(Value::Map(vec![("bad".into(), Value::Int(123))])),
        ),
    ];
    let spans: Vec<Span> = (0..decls.len())
        .map(|i| Span::new(i * 10, i * 10 + 9))
        .collect();
    let declared: Vec<_> = decls
        .into_iter()
        .zip(spans.iter().copied())
        .map(|(d, s)| Declared::new(d, s))
        .collect();
    let mut errors = Vec::new();
    check_ctmt_declarations(&declared, &mut errors);

    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownElement { .. }))
    );
    assert_eq!(errors.len(), 4);
    assert!(errors.iter().any(|e| matches!(
        e,
        Diagnostic::NotForThisHost {
            declaration: "use",
            ..
        }
    )));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::DuplicateSingleton { name, first, duplicate } if name == "meta" && *first == spans[3] && *duplicate == spans[4])
    ));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::InvalidVersion { element, found, .. } if element == "kind" && found == "\"abc\"")
    ));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::InvalidDefinition { declaration: "macros", name, .. } if name == "bad")
    ));
}
