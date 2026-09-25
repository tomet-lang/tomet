use super::*;

/// A list item's trailing `{value}` attrs, if any -- mirrors how
/// `Element::list_item` stores them under `value` as `ElementValue::Data`.
fn item_attrs(item: &Element) -> Option<Value> {
    match &item.value {
        Some(v) => v.as_data(),
        _ => None,
    }
}

#[test]
fn parses_list() {
    let doc = parse_document("- one\n- two\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(list) => {
            assert_eq!(list_ordered(list), Some(false));
            let items = list_items(list);
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].content, Some(vec![Inline::Text("one".into())]));
            assert_eq!(items[1].content, Some(vec![Inline::Text("two".into())]));
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn parses_list_value_markers_and_trailing_attrs() {
    let doc = parse_document("- (T) todo {tag: dev}\n- (\"?\") question {id: task1}\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(list) => {
            assert_eq!(list_ordered(list), Some(false));
            let items = list_items(list);
            assert_eq!(items.len(), 2);

            assert_eq!(items[0].content, Some(vec![Inline::Text("todo".into())]));
            assert_eq!(items[0].args, Some(Value::String("T".into())));
            assert_eq!(
                item_attrs(&items[0]),
                Some(Value::Map(vec![(
                    "tag".into(),
                    Value::String("dev".into())
                )]))
            );

            assert_eq!(
                items[1].content,
                Some(vec![Inline::Text("question".into())])
            );
            assert_eq!(items[1].args, Some(Value::String("?".into())));
            assert_eq!(
                item_attrs(&items[1]),
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
        Block::Element(list) => {
            let items = list_items(list);
            assert_eq!(items.len(), 1, "{bare:?}");
            assert_eq!(item_attrs(&items[0]), None, "{bare:?}");
            match items[0].content.as_deref() {
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
            Block::Element(list) => {
                let items = list_items(list);
                assert_eq!(items.len(), 1, "{src:?}");
                assert_eq!(
                    item_attrs(&items[0]),
                    Some(Value::Map(vec![(
                        "id".into(),
                        Value::String("breakfast".into())
                    )])),
                    "{src:?}"
                );
                // The connector (` :`) left no trace in the item's
                // own content -- just the `@link` element, nothing
                // else.
                match items[0].content.as_deref() {
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
    // The `docs/spec/syntax.tmt` `##[ コネクト ]` example
    // this feature implements (`- () xxxxxx :{}`) uses an *empty*
    // `{}` for its item-level attrs -- `heading::parse_braced_value`
    // (shared by heading and list-item attrs) used to have no
    // special case for that, unlike `parse_paren_value`/
    // `element.rs::parse_value_group` right next to it, and errored
    // ("expected a value") instead of producing an empty map,
    // silently falling through to treating the whole line as
    // ordinary text with no attrs at all.
    let doc = parse_document("- () xxxxxx :{}\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(list) => {
            let items = list_items(list);
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].content, Some(vec![Inline::Text("xxxxxx".into())]));
            assert_eq!(item_attrs(&items[0]), Some(Value::Map(vec![])));
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn list_marker_supports_explicit_key_value() {
    let doc = parse_document("- (color: red, priority: high) content\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(list) => {
            assert_eq!(
                list_items(list)[0].args,
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
        Block::Element(list) => {
            assert_eq!(
                list_items(list)[0].args,
                Some(Value::String("12:01".into()))
            );
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
        Block::Element(list) => {
            let items = list_items(list);
            assert_eq!(items[0].args, None);
            let content = items[0].content.as_ref().expect("content");
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
        Block::Element(list) => {
            let items = list_items(list);
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
        Block::Element(list) => {
            assert_eq!(list_ordered(list), Some(true));
            let items = list_items(list);
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].content, Some(vec![Inline::Text("one".into())]));
            assert_eq!(items[1].content, Some(vec![Inline::Text("two".into())]));
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn mixed_list_markers_split_into_separate_lists() {
    let doc = parse_document("- one\n-. two\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Element(l1), Block::Element(l2))
            if list_ordered(l1) == Some(false) && list_ordered(l2) == Some(true) => {}
        other => panic!("expected two separate lists, got {other:?}"),
    }
}
