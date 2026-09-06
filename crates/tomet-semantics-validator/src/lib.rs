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
                unbound_namespace: unknown.unbound_namespace,
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

    check_singletons_and_regions(doc, bindings, &mut errors);
    check_retired_settings_keys(doc, &mut errors);
    check_arguments(doc, bindings, &mut errors);

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

/// Enforces the `singleton` and `region` axes.
///
/// Both were declared in `docs/spec/builtin-settings.tmt` and read by
/// nothing, under a `placement:` key that also carried the block/inline
/// rule -- three concepts in one word, which is why that key is being
/// retired in favour of `display`, `region` and `singleton`.
///
/// The preamble is the run of preamble-region elements at the top of the
/// document. The first top-level block that is not one of them ends it,
/// and anything preamble-only after that point, or nested inside
/// content, is out of place.
fn check_singletons_and_regions(
    doc: &Document,
    bindings: &Bindings,
    errors: &mut Vec<ValidationError>,
) {
    use tomet_ast::Block;
    use tomet_semantics::Region;

    let constraints = |el: &tomet_ast::Element| -> Option<(String, bool, Region)> {
        let name = el.sigil.name()?;
        let kind = classify_in(el, bindings).ok()?;
        // A vocabulary says what it wants; `std` has its own table.
        let (singleton, region) = match bindings.declaration(name) {
            Some(decl) => (decl.singleton, decl.region),
            None => (
                tomet_semantics::builtin_singleton(&kind),
                tomet_semantics::builtin_region(&kind),
            ),
        };
        Some((name.to_string(), singleton, region))
    };

    let mut in_preamble = true;
    let mut seen: Vec<(String, tomet_ast::Span)> = Vec::new();

    // `in_place` is passed in rather than captured: the closure would
    // otherwise borrow `in_preamble` for the whole loop.
    let mut visit = |el: &tomet_ast::Element, in_place: bool| {
        let Some((name, singleton, region)) = constraints(el) else {
            return;
        };
        if singleton {
            if let Some((_, first)) = seen.iter().find(|(seen_name, _)| *seen_name == name) {
                errors.push(ValidationError::DuplicateSingleton {
                    name: name.clone(),
                    first: *first,
                    duplicate: el.span,
                });
            } else {
                seen.push((name.clone(), el.span));
            }
        }
        if region == Region::Preamble && !in_place {
            errors.push(ValidationError::OutsidePreamble {
                name,
                span: el.span,
            });
        }
    };

    for block in &doc.blocks {
        match block {
            Block::Element(el) => {
                let still_preamble = constraints(el)
                    .map(|(_, _, region)| region == Region::Preamble)
                    .unwrap_or(false);
                visit(el, in_preamble);
                if !still_preamble {
                    in_preamble = false;
                }
            }
            other => {
                in_preamble = false;
                tomet_tree::for_each_element_in_block(other, |el| visit(el, false));
            }
        }
    }
}

/// Checks each element's `(args)` against the `@param`s its vocabulary
/// declares.
///
/// Silent for anything `std` owns, and for a custom element whose
/// declaration carries no `@args`. An absent `@args` says nothing about
/// the arguments rather than saying there are none -- that second
/// statement is `@data`'s `open: false`, which has no `(args)` twin yet,
/// and inventing one here would decide it by accident.
///
/// The arguments are normalized first, so a positional value has already
/// been moved onto the slot its `@param` declares. Without that,
/// `@deck.card(3)` would report the sentinel-keyed entry as an unknown
/// argument named `""`.
fn check_arguments(doc: &Document, bindings: &Bindings, errors: &mut Vec<ValidationError>) {
    use tomet_ast::Value;

    tomet_tree::for_each_element(doc, |el| {
        let Some(name) = el.sigil.name() else { return };
        let Some(decl) = bindings.declaration(name) else {
            return;
        };
        if decl.params.is_empty() {
            return;
        }

        let args = tomet_semantics::normalized_element_args_in(el, bindings);
        let entries: &[(String, Value)] = match args.as_ref() {
            Some(Value::Map(entries)) => entries,
            // A non-map `(args)` survives normalization only when the
            // element has no positional slot to put it on, which cannot
            // happen here: `decl.params` is non-empty. Anything else is
            // `(args)` being absent.
            _ => &[],
        };

        for (key, _) in entries {
            if !key.is_empty() && decl.param(key).is_none() {
                errors.push(ValidationError::UnknownArgument {
                    element: name.to_string(),
                    argument: key.clone(),
                    span: el.span,
                });
            }
        }
        for param in decl.params.iter().filter(|p| p.required) {
            if !entries.iter().any(|(k, _)| *k == param.name) {
                errors.push(ValidationError::MissingRequiredArgument {
                    element: name.to_string(),
                    argument: param.name.clone(),
                    span: el.span,
                });
            }
        }
    });
}

/// Reports `@settings`/`@config` keys that have been retired.
///
/// One key so far, `elements:`, plus the `types:` map that sat beside it.
/// Between them they described a custom element -- its arguments, whether
/// it was a singleton, which shape it took -- which is a `@vocabulary`
/// question now. See [`ValidationError::RetiredSettingsKey`] for why this
/// is an error and not a silent skip.
///
/// Top-level blocks only, matching what a settings loader would actually
/// read: an `@settings` buried in a paragraph is already reported by the
/// region rule above.
fn check_retired_settings_keys(doc: &Document, errors: &mut Vec<ValidationError>) {
    use tomet_ast::{Block, Value};

    const RETIRED: [&str; 2] = ["elements", "types"];

    for block in &doc.blocks {
        let Block::Element(el) = block else { continue };
        if !el.sigil.is_bare_named("settings") && !el.sigil.is_bare_named("config") {
            continue;
        }
        let Some(Value::Map(entries)) = tomet_semantics::embedded::element_data(el) else {
            continue;
        };
        for key in RETIRED {
            if entries.iter().any(|(k, _)| k == key) {
                errors.push(ValidationError::RetiredSettingsKey {
                    key: key.to_string(),
                    span: el.span,
                });
            }
        }
    }
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
            ValidationError::DuplicateSingleton { name, .. } if name == "meta"
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
            ValidationError::DuplicateSingleton { name, .. } if name == "spread"
        ));
    }

    /// The preamble is the run of preamble-region elements at the top.
    /// The first block that is not one of them ends it.
    #[test]
    fn a_preamble_element_after_the_body_is_reported() {
        let doc = parse("@kind(note)\n\n#[ T ]\n\n@use(deck)\n");
        let errors = validate_document(&doc);
        assert!(
            errors.iter().any(
                |e| matches!(e, ValidationError::OutsidePreamble { name, .. } if name == "use")
            ),
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
                ValidationError::DuplicateSingleton { .. }
                    | ValidationError::OutsidePreamble { .. }
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
                ValidationError::UnknownArgument { argument, .. } if argument == "colour"
            )),
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
                ValidationError::MissingRequiredArgument { argument, .. } if argument == "id"
            )),
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
                .any(|e| matches!(e, ValidationError::UnknownArgument { .. })),
            "{errors:?}"
        );
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
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::RetiredSettingsKey { key, .. } if key == "elements")),
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
            .find(
                |e| matches!(e, ValidationError::RetiredSettingsKey { key, .. } if key == "types"),
            )
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
                .any(|e| matches!(e, ValidationError::RetiredSettingsKey { .. })),
            "{errors:?}"
        );
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
