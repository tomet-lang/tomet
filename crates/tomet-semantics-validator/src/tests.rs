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
/// so two `@meta` -- or two `@kind`, which is two answers to what the
/// document is -- passed every check this project had.
#[test]
fn a_second_singleton_is_reported() {
    let doc = parse("@kind(note)\n@meta{a: 1}\n@meta{b: 2}\n\n#[ T ]\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateSingleton { name, .. } if name == "meta"
    ));
}

/// A vocabulary's own `singleton: true` is enforced the same way. The
/// builtin table and a declaration are two sources for one rule, not
/// two rules.
#[test]
fn a_vocabularys_singleton_is_enforced_too() {
    let vocab = tomet_parser::parse_document(
        "@kind(vocabulary)\n@vocabulary(deck)\n\n@element(spread){ singleton: true }[ One. ]\n",
    )
    .expect("vocabulary parses");
    let bindings = Bindings {
        kind: tomet_semantics::Vocabulary::from_document(&vocab),
        ..Bindings::default()
    };

    let doc = parse("@kind(deck)\n\n#[ T ]\n\n@spread{}\n@spread{}\n");
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
    let doc =
        parse("@section[ @card(title:\"a\")[ x ] @heading[ y ] ]:rule(allow:list(card))\n");
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
        "@kind(vocabulary)\n@vocabulary(ns)\n\n@element(mycard){}\n",
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
    let doc = parse("@kind(note)\n\n#[ T ]\n\n@use(deck)\n");
    let errors = validate_document(&doc);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::OutsidePreamble { name, .. } if name == "use")),
        "{errors:?}"
    );
}

/// Several `@use` in the preamble is fine -- you bring in several
/// vocabularies. That `use` is preamble-only and not a singleton is
/// what proves the two axes are independent.
#[test]
fn several_preamble_elements_are_fine_when_they_are_not_singletons() {
    let doc = parse("@kind(note)\n@use(deck)\n@use(cards)\n\n#[ T ]\n");
    let errors = validate_document(&doc);
    assert!(
        !errors.iter().any(|e| matches!(
            e,
            Diagnostic::DuplicateSingleton { .. } | Diagnostic::OutsidePreamble { .. }
        )),
        "{errors:?}"
    );
}

/// A vocabulary with a declared parameter list.
fn deck_with_params() -> Bindings {
    let vocab = tomet_semantics::Vocabulary::from_document(&parse(
        r#"@kind(vocabulary)
@vocabulary(deck){}

@element(card){
  @args{
    @param(id){ positional: true, required: true }[ 通し番号。 ]
    @param(tags){}[ タグ。 ]
  }
}[ カード。 ]

@element(plain){}[ 宣言なし。 ]
"#,
    ))
    .expect("a vocabulary");
    Bindings::for_document(&parse("@kind(deck)\n"), [vocab])
}

/// A positional argument fills the slot its `@param` declares, so it
/// is not reported as an unknown argument keyed `""`.
#[test]
fn a_positional_argument_lands_on_its_declared_slot() {
    let doc = parse("@kind(deck)\n\n@card(3)\n");
    let errors = validate_document_with(&doc, &deck_with_params());
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn an_undeclared_argument_is_reported() {
    let doc = parse("@kind(deck)\n\n@card(id: 3, colour: red)\n");
    let errors = validate_document_with(&doc, &deck_with_params());
    assert!(
        errors.iter().any(|e| matches!(
            e,
            Diagnostic::UnknownArgument { argument, .. } if argument == "colour"
        )),
        "{errors:?}"
    );
}

/// A vocabulary with `@content{allow:}` declared, for
/// `check_content_shape`'s own tests.
fn deck_with_content_allow() -> Bindings {
    let vocab = tomet_semantics::Vocabulary::from_document(&parse(
        r#"@kind(vocabulary)
@vocabulary(deck){}

@element(list){
  display: block
  // `item` resolves bare here (the document's own `@kind(deck)`), which
  // `Bindings::classify` normalizes to `deck.item` even though it's
  // written bare -- same pre-existing quirk `check_one_rule`'s own doc
  // comment notes for `:rule(allow:...)`, so the allow-list has to name
  // it the same way.
  @content{ allow: list(deck.item) }
}[ カードの一覧。 ]

@element(item){
  display: block
  @content{ allow: inline }
}[ 一覧の一項目。 ]
"#,
    ))
    .expect("a vocabulary");
    Bindings::for_document(&parse("@kind(deck)\n"), [vocab])
}

#[test]
fn content_allowed_by_name_is_clean() {
    let doc = parse("@kind(deck)\n\n@list[\n  @item[ one ]\n  @item[ two ]\n]\n");
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
    let doc = parse("@kind(deck)\n\n@list[\n  @em[ not an item ]\n]\n");
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
    let doc = parse("@kind(deck)\n\n@item[\n  one\n\n  two\n]\n");
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
    let doc = parse("@kind(deck)\n\n@card(tags: a)\n");
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
    let doc = parse("@meta(icon: @doc.icon(pkg:\"lucide\"))\n");
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
    let doc = parse("@meta(icon: @doc.icon(\"triangle\", pkg:\"lucide\"))\n");
    assert_eq!(validate_document(&doc), vec![]);
}

#[test]
fn an_unregistered_name_under_doc_is_still_unknown_when_embedded() {
    let doc = parse("@meta(icon: @doc.glyph(\"triangle\"))\n");
    let errors = validate_document(&doc);
    assert!(
        errors.iter().any(
            |e| matches!(e, Diagnostic::UnknownElement { name, .. } if name == "doc.glyph")
        ),
        "{errors:?}"
    );
}

/// An element whose declaration has no `@args` says nothing about its
/// arguments. Saying it takes none is a different statement, and one
/// the vocabulary has no spelling for yet.
#[test]
fn an_element_with_no_args_declaration_is_not_checked() {
    let doc = parse("@kind(deck)\n\n@plain(anything: 1)\n");
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
        "@kind(note)\n\n@draft[ ローカル保存の理由 ]\n\n地の文に @fixme[ 直す ] も置ける。\n",
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
    let doc = parse("@kind(note)\n\n@conflict(a: [ ローカル ], b: [ リモート ])\n");
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
    let doc =
        parse("@kind(note)\n\n@draft[ 節まるごと ]\n\n文の途中の @draft[ 一語 ] も可。\n");
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
    let doc = parse("@kind(note)\n\n@nonesuch{}\n");
    let diagnostics = validate_document(&doc);
    assert!(diagnostics.iter().all(|d| d.severity() == Severity::Error));
}

/// `elements:` described a custom element in `@settings`; a
/// `@vocabulary` document says all of it now, so the settings copy is
/// reported rather than skipped -- skipping is how it survived being
/// read by nothing.
#[test]
fn a_retired_settings_key_is_reported() {
    let doc = parse(
        "@kind(settings)\n@settings(format:json)+++\n\
         { \"elements\": { \"bookmark\": { \"singleton\": false } } }\n+++\n",
    );
    let errors = validate_document(&doc);
    assert!(
        errors.iter().any(
            |e| matches!(e, Diagnostic::RetiredSettingsKey { key, .. } if key == "elements")
        ),
        "{errors:?}"
    );
}

/// `types:` goes with it, and its message has to say there is nowhere
/// to move it to rather than inventing a destination.
#[test]
fn the_types_map_is_retired_too_and_says_it_has_no_replacement() {
    let doc = parse(
        "@kind(settings)\n@settings(format:json)+++\n\
         { \"types\": { \"bookmark\": { \"style\": \"one_line\" } } }\n+++\n",
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

/// The live surface is untouched. `format`, `macros` and the path
/// lists are what `tomet-config` actually reads.
#[test]
fn a_settings_document_using_only_live_keys_is_clean() {
    let doc = parse(
        "@kind(config)\n@config(format:json)+++\n\
         { \"format\": { \"callout\": { \"style\": { \"content\": \"block\" } } },\n\
         \"macros\": { \"gh\": \"https://example.com/${1}\" } }\n+++\n",
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
    let doc = parse("=[ one ]#(42)\n=[ two ]#(42)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        &errors[0],
        Diagnostic::DuplicateId { id, .. } if id == "42"
    ));
    assert_eq!(
        errors[0].to_string(),
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
