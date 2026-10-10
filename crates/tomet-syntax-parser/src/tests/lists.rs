use super::*;
use tomet_semantics::classify_std_lenient;

#[test]
fn parses_list() {
    let doc = parse_document("- one\n- two\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            assert!(!list.ordered);
            let items = &list.items;
            assert_eq!(items.len(), 2);
            assert_eq!(
                items[0].element.content,
                wrap(vec![Inline::Text("one".into())])
            );
            assert_eq!(
                items[1].element.content,
                wrap(vec![Inline::Text("two".into())])
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn parses_list_value_markers_and_trailing_attrs() {
    let doc = parse_document("- (T) todo {tag: dev}\n- (\"?\") question {id: task1}\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            assert!(!list.ordered);
            let items = &list.items;
            assert_eq!(items.len(), 2);

            assert_eq!(
                items[0].element.content,
                wrap(vec![Inline::Text("todo".into())])
            );
            assert_eq!(items[0].element.args, Some(Value::String("T".into())));
            assert_eq!(
                item_attrs(&items[0].element),
                Some(Value::Map(vec![(
                    "tag".into(),
                    Value::String("dev".into())
                )]))
            );

            assert_eq!(
                items[1].element.content,
                wrap(vec![Inline::Text("question".into())])
            );
            assert_eq!(items[1].element.args, Some(Value::String("?".into())));
            assert_eq!(
                item_attrs(&items[1].element),
                Some(Value::Map(vec![(
                    "id".into(),
                    Value::String("task1".into())
                )]))
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn list_item_ending_in_an_element_does_not_error_on_a_trailing_brace() {
    // `element.rs::parse_element`'s main loop always lets an element
    // claim an adjacent *bare*, colon-less `{value}` group as its
    // own -- just whitespace/one blank line, no colon needed --
    // *before* the list item's own trailing-attrs guess
    // (`list.rs::peek_trailing_attrs`) ever gets a say. Previously
    // (before `allow_colon_connect` existed) this shape failed
    // outright with "expected '{'" regardless of whether a colon was
    // present: the attrs guess found the line's one `{`, assumed it
    // belonged to the item, told inline parsing to stop right before
    // it, but the element inside kept going and swallowed straight
    // past that point anyway, leaving nothing at the assumed
    // position to re-parse as the item's own attrs.
    //
    // Now the two shapes mean different things, on purpose (see
    // `element::parse_element`'s `allow_colon_connect` doc comment,
    // and `docs/spec/syntax.tmt`'s `##[ コネクト ]`,
    // which had flagged exactly this as an unimplemented idea): a
    // *bare* `{...}` still always belongs to the element (matches
    // how the same input already behaves with no list item involved
    // at all), but a *colon-prefixed* `:{...}` is reserved for the
    // item itself, specifically so `id:breakfast` here can be the
    // item's own metadata rather than forced onto `@link`.
    let bare = "- @link(ref:x) {id:breakfast}\n";
    let doc = parse_document(bare).unwrap_or_else(|e| panic!("{bare:?} failed: {e}"));
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items.len(), 1, "{bare:?}");
            assert_eq!(item_attrs(&items[0].element), None, "{bare:?}");
            match items[0].element.content.as_deref().map(first_para) {
                Some([Inline::Element(link)]) => {
                    assert_eq!(link.sigil, Sigil::named("link"), "{bare:?}");
                    assert_eq!(
                        link.value,
                        Some(ElementValue::from_map(Value::Map(vec![(
                            "id".into(),
                            Value::String("breakfast".into())
                        )]))),
                        "{bare:?}"
                    );
                }
                other => panic!("{bare:?}: expected a single @link element, got {other:?}"),
            }
        }
        other => panic!("{bare:?}: expected list, got {other:?}"),
    }

    for src in [
        "- @link(ref:x) :{id:breakfast}\n",
        "- @link(ref:@other) :{id:breakfast}\n",
    ] {
        let doc = parse_document(src).unwrap_or_else(|e| panic!("{src:?} failed: {e}"));
        match &doc.blocks[0] {
            Block::List(list) => {
                let items = &list.items;
                assert_eq!(items.len(), 1, "{src:?}");
                assert_eq!(
                    item_attrs(&items[0].element),
                    Some(Value::Map(vec![(
                        "id".into(),
                        Value::String("breakfast".into())
                    )])),
                    "{src:?}"
                );
                // The connector (` :`) left no trace in the item's
                // own content -- just the `@link` element, nothing
                // else.
                match items[0].element.content.as_deref().map(first_para) {
                    Some([Inline::Element(link)]) => {
                        assert_eq!(link.sigil, Sigil::named("link"), "{src:?}");
                        assert_eq!(link.value, None, "{src:?}");
                    }
                    other => panic!("{src:?}: expected a single @link element, got {other:?}"),
                }
            }
            other => panic!("{src:?}: expected list, got {other:?}"),
        }
    }
}

#[test]
fn list_item_colon_connect_supports_an_empty_braced_value() {
    // The `docs/spec/syntax.tmt` `##[ コネクト ]` example this
    // feature implements (`- () xxxxxx :{}`) uses an *empty* `{}` for
    // its item-level attrs -- `element.rs::parse_value_group` (the same
    // reader a trailing sugar connect's `{value}` now goes through)
    // already special-cases an empty body into an empty map rather than
    // erroring, so this just confirms the sugar path inherits that.
    let doc = parse_document("- () xxxxxx :{}\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0].element.content,
                wrap(vec![Inline::Text("xxxxxx".into())])
            );
            assert_eq!(item_attrs(&items[0].element), Some(Value::Map(vec![])));
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn list_marker_supports_explicit_key_value() {
    let doc = parse_document("- (color: red, priority: high) content\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(
                items[0].element.args,
                Some(Value::Map(vec![
                    ("color".into(), Value::String("red".into())),
                    ("priority".into(), Value::String("high".into())),
                ]))
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn list_marker_quoted_scalar_avoids_colon_misparse() {
    let doc = parse_document("- (\"12:01\") woke up\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items[0].element.args, Some(Value::String("12:01".into())));
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn bracket_after_a_list_marker_is_the_content_group() {
    // `[...]` is the item's content group -- the same group `@name`
    // takes -- not a checkbox and not literal text. `args` stays
    // `None` because no `(...)` was written. Trailing text joins the
    // item rather than falling out as a block of its own, matching
    // `@x[T] content`, where both halves stay in one paragraph.
    let doc = parse_document("- [T] content\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items[0].element.args, None);
            let content = first_para(items[0].element.content.as_ref().expect("content"));
            let text: String = content
                .iter()
                .map(|inline| match inline {
                    Inline::Text(t) => t.value.as_str(),
                    other => panic!("expected text, got {other:?}"),
                })
                .collect();
            assert_eq!(text, "T content");
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn a_bracketed_list_item_may_span_lines() {
    // The bracket-less sugar is single-line by design; a group is the
    // explicit trigger that lets an item spread out.
    let doc = parse_document("- ()[ first\n  second ]\n- ()[ third ]\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items.len(), 2, "both items belong to one list");
        }
        other => panic!("expected list, got {other:?}"),
    }
    assert_eq!(doc.blocks.len(), 1, "no stray paragraph split off");
}

#[test]
fn parses_ordered_list() {
    let doc = parse_document("-. one\n-. two\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            assert!(list.ordered);
            let items = &list.items;
            assert_eq!(items.len(), 2);
            assert_eq!(
                items[0].element.content,
                wrap(vec![Inline::Text("one".into())])
            );
            assert_eq!(
                items[1].element.content,
                wrap(vec![Inline::Text("two".into())])
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn full_form_list_item_supports_a_bare_trailing_connect() {
    // `ConnectMode::Bare`: a full-form item's own `:(...)`/`:{...}` now
    // merges into its own `args`/`value`, the same arms `@name` already
    // uses for this -- previously `allow_colon_connect` was hardcoded
    // `false` for the item's own groups, so none of these forms attached
    // anywhere; the trailing `:...` became literal text instead.
    let doc = parse_document("- [ content ]:(x: 1)\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(
                items[0].element.args,
                Some(Value::Map(vec![("x".into(), Value::Int(1))]))
            );
        }
        other => panic!("expected list, got {other:?}"),
    }

    let doc = parse_document("- [ content ]:{y: 2}\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(
                item_attrs(&items[0].element),
                Some(Value::Map(vec![("y".into(), Value::Int(2))]))
            );
        }
        other => panic!("expected list, got {other:?}"),
    }

    // Combined `:(){}`: the colon only has to precede the first group --
    // the second is claimed by the item's own, still-empty slot the same
    // way it would be with no colon at all (see `parse_groups_with_pipe_stack`'s
    // colon branch).
    let doc = parse_document("- [ content ]:(x: 1){y: 2}\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(
                items[0].element.args,
                Some(Value::Map(vec![("x".into(), Value::Int(1))]))
            );
            assert_eq!(
                item_attrs(&items[0].element),
                Some(Value::Map(vec![("y".into(), Value::Int(2))]))
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn full_form_list_item_supports_a_named_connect() {
    // A list item's own groups now take the full connect branch, same as
    // `@name`'s or a heading's -- `element_list_item` carries `connects`
    // too, so `:rule(...)` attaches to the item instead of becoming
    // literal trailing text.
    let doc = parse_document("- [ content ]:rule(allow: list(card))\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items[0].element.connects.len(), 1);
            assert_eq!(items[0].element.connects[0].sigil, Sigil::named("rule"));
            let text: String = first_para(items[0].element.content.as_ref().expect("content"))
                .iter()
                .map(|inline| match inline {
                    Inline::Text(t) => t.value.as_str(),
                    other => panic!("expected text, got {other:?}"),
                })
                .collect();
            assert_eq!(text, "content");
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn mixed_list_markers_split_into_separate_lists() {
    let doc = parse_document("- one\n-. two\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::List(l1), Block::List(l2)) if !l1.ordered && l2.ordered => {}
        other => panic!("expected two separate lists, got {other:?}"),
    }
}

#[test]
fn combine_notation_reads_a_named_item_with_content_and_connects() {
    // `-@name(...)` -- the combine notation. `item.element` ends up a
    // real, named `Element`, exactly like a standalone `@task(...)`
    // would produce, with its own `[content]`, bare `:(){}` merge, and
    // named `:rule(...)` connect all landing on it directly -- no
    // marker, no second grammar.
    let doc = parse_document("-@task()[content]:rule(allow:list(card))\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items.len(), 1);
            let item = &items[0].element;
            assert_eq!(item.sigil, Sigil::named("task"));
            assert_eq!(item.placement, Placement::Block);
            assert_eq!(item.content, wrap(vec![Inline::Text("content".into())]));
            assert_eq!(item.connects.len(), 1);
            assert_eq!(item.connects[0].sigil, Sigil::named("rule"));
            assert_eq!(items[0].sublist, None);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn combine_notation_supports_a_bare_trailing_connect() {
    // `:(){}` merges into the combine item's own `args`/`value`, the same
    // slot a standalone `@task():(x: 1){y: 2}` would use.
    let doc = parse_document("-@task():(x: 1){y: 2}\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let item = &list.items[0].element;
            assert_eq!(
                item.args,
                Some(Value::Map(vec![("x".into(), Value::Int(1))]))
            );
            assert_eq!(
                item_attrs(item),
                Some(Value::Map(vec![("y".into(), Value::Int(2))]))
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn combine_notation_has_no_naming_constraint() {
    // `-@task()` and a standalone, top-level `@task()` classify
    // identically -- position (list item vs. document block) and naming
    // (what kind of thing it is) are orthogonal axes. Asserted via
    // `classify_std_lenient` since `classify_in` (which also resolves a
    // vocabulary's own bindings) lives in `tomet-semantics-validator`,
    // a layer above this crate.
    let combine_doc = parse_document("-@task()\n").unwrap();
    let Block::List(list) = &combine_doc.blocks[0] else {
        panic!("expected list");
    };
    let combine_el = &list.items[0].element;

    let standalone_doc = parse_document("@task()\n").unwrap();
    let Block::Element(standalone_el) = &standalone_doc.blocks[0] else {
        panic!("expected element");
    };

    assert_eq!(combine_el.sigil, standalone_el.sigil);
    assert_eq!(
        classify_std_lenient(combine_el),
        classify_std_lenient(standalone_el)
    );
}

#[test]
fn a_nested_list_becomes_the_items_own_sublist() {
    // `- a\n  - b\n` -- the indented `- b` is `a`'s own `sublist`
    // (`Option<List>`), not a block inside `a`'s own `content`.
    let doc = parse_document("- a\n  - b\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            assert_eq!(list.items.len(), 1);
            let item = &list.items[0];
            assert_eq!(item.element.content, wrap(vec![Inline::Text("a".into())]));
            let sub = item.sublist.as_ref().expect("nested sub-list");
            assert_eq!(sub.items.len(), 1);
            assert_eq!(
                sub.items[0].element.content,
                wrap(vec![Inline::Text("b".into())])
            );
            assert_eq!(sub.items[0].sublist, None);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn multiple_levels_of_nesting_still_work() {
    let doc = parse_document("- a\n  - b\n    - c\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let a = &list.items[0];
            assert_eq!(a.element.content, wrap(vec![Inline::Text("a".into())]));
            let level_b = a.sublist.as_ref().expect("level b sub-list");
            let b = &level_b.items[0];
            assert_eq!(b.element.content, wrap(vec![Inline::Text("b".into())]));
            let level_c = b.sublist.as_ref().expect("level c sub-list");
            let c = &level_c.items[0];
            assert_eq!(c.element.content, wrap(vec![Inline::Text("c".into())]));
            assert_eq!(c.sublist, None);
        }
        other => panic!("expected list, got {other:?}"),
    }
}
