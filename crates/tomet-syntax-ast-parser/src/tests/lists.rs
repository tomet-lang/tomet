use super::*;

#[test]
fn parses_list() {
    let doc = parse_file("- one\n- two\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            assert!(!list.ordered);
            let items = &list.items;
            assert_eq!(items.len(), 2);
            assert_eq!(
                items[0].content,
                wrap(vec![Inline::Text("one".into())]).unwrap()
            );
            assert_eq!(
                items[1].content,
                wrap(vec![Inline::Text("two".into())]).unwrap()
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn parses_list_value_markers_and_trailing_attrs() {
    let doc = parse_file("- (T) todo {tag: dev}\n- (\"?\") question {id: task1}\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            assert!(!list.ordered);
            let items = &list.items;
            assert_eq!(items.len(), 2);

            assert_eq!(
                items[0].content,
                wrap(vec![Inline::Text("todo".into())]).unwrap()
            );
            assert_eq!(items[0].data.args, Some(Value::String("T".into())));
            assert_eq!(
                item_attrs(&items[0]),
                Some(Value::Map(vec![(
                    "tag".into(),
                    Value::String("dev".into())
                )]))
            );

            assert_eq!(
                items[1].content,
                wrap(vec![Inline::Text("question".into())]).unwrap()
            );
            assert_eq!(items[1].data.args, Some(Value::String("?".into())));
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
    // An element always claims an adjacent *bare*, colon-less `{value}`
    // group as its own -- just whitespace/one blank line, no colon
    // needed -- before the list item's own trailing groups get a say.
    //
    // The two shapes mean different things, on purpose (see
    // `docs/spec/syntax.tmt`'s `##[ コネクト ]`): a
    // *bare* `{...}` still always belongs to the element (matches
    // how the same input already behaves with no list item involved
    // at all), but a *colon-prefixed* `::{...}` is reserved for the
    // item itself, specifically so `id:breakfast` here can be the
    // item's own metadata rather than forced onto `@link`.
    let bare = "- @link(ref:x) {id:breakfast}\n";
    let doc = parse_file(bare).unwrap_or_else(|e| panic!("{bare:?} failed: {e}"));
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items.len(), 1, "{bare:?}");
            assert_eq!(item_attrs(&items[0]), None, "{bare:?}");
            match Some(first_para(&items[0].content)) {
                Some([Inline::Element(link)]) => {
                    assert_eq!(link.name, Name::bare("link"), "{bare:?}");
                    assert_eq!(
                        link.data.value,
                        Some(Group::from_map(Value::Map(vec![(
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
        "- @link(ref:x) ::{id:breakfast}\n",
        "- @link(ref:@other) ::{id:breakfast}\n",
    ] {
        let doc = parse_file(src).unwrap_or_else(|e| panic!("{src:?} failed: {e}"));
        match &doc.body[0] {
            Block::List(list) => {
                let items = &list.items;
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
                match Some(first_para(&items[0].content)) {
                    Some([Inline::Element(link)]) => {
                        assert_eq!(link.name, Name::bare("link"), "{src:?}");
                        assert_eq!(link.data.value, None, "{src:?}");
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
    // feature implements (`- () xxxxxx ::{}`) uses an *empty* `{}` for
    // its item-level attrs -- `element.rs::parse_value_group` (the same
    // reader a trailing sugar connect's `{value}` now goes through)
    // already special-cases an empty body into an empty map rather than
    // erroring, so this just confirms the sugar path inherits that.
    let doc = parse_file("- () xxxxxx ::{}\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items.len(), 1);
            assert_eq!(
                items[0].content,
                wrap(vec![Inline::Text("xxxxxx".into())]).unwrap()
            );
            assert_eq!(item_attrs(&items[0]), Some(Value::Map(vec![])));
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn list_marker_supports_explicit_key_value() {
    let doc = parse_file("- (color: red, priority: high) content\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(
                items[0].data.args,
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
    let doc = parse_file("- (\"12:01\") woke up\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items[0].data.args, Some(Value::String("12:01".into())));
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
    let doc = parse_file("- [T] content\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items[0].data.args, None);
            let content = first_para(&items[0].content);
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
    let doc = parse_file("- ()[ first\n  second ]\n- ()[ third ]\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items.len(), 2, "both items belong to one list");
        }
        other => panic!("expected list, got {other:?}"),
    }
    assert_eq!(doc.body.len(), 1, "no stray paragraph split off");
}

#[test]
fn parses_ordered_list() {
    let doc = parse_file("-. one\n-. two\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            assert!(list.ordered);
            let items = &list.items;
            assert_eq!(items.len(), 2);
            assert_eq!(
                items[0].content,
                wrap(vec![Inline::Text("one".into())]).unwrap()
            );
            assert_eq!(
                items[1].content,
                wrap(vec![Inline::Text("two".into())]).unwrap()
            );
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn full_form_list_item_supports_a_bare_trailing_connect() {
    // `ConnectMode::Bare`: a full-form item's own `::(...)`/`::{...}` now
    // merges into its own `args`/`value`, the same arms `@name` already
    // uses for this -- previously `allow_colon_connect` was hardcoded
    // `false` for the item's own groups, so none of these forms attached
    // anywhere; the trailing `:...` became literal text instead.
    let doc = parse_file("- [ content ]::(x: 1)\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(
                items[0].data.args,
                Some(Value::Map(vec![("x".into(), Value::Int(1))]))
            );
        }
        other => panic!("expected list, got {other:?}"),
    }

    let doc = parse_file("- [ content ]::{y: 2}\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(
                item_attrs(&items[0]),
                Some(Value::Map(vec![("y".into(), Value::Int(2))]))
            );
        }
        other => panic!("expected list, got {other:?}"),
    }

    // Combined `::(){}`: the colon only has to precede the first group --
    // the second is claimed by the item's own, still-empty slot the same
    // way it would be with no colon at all (see `parse_groups_with_pipe_stack`'s
    // colon branch).
    let doc = parse_file("- [ content ]::(x: 1){y: 2}\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(
                items[0].data.args,
                Some(Value::Map(vec![("x".into(), Value::Int(1))]))
            );
            assert_eq!(
                item_attrs(&items[0]),
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
    // too, so `::rule(...)` attaches to the item instead of becoming
    // literal trailing text.
    let doc = parse_file("- [ content ]::rule(allow: (card))\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items[0].connects.len(), 1);
            assert_eq!(items[0].connects[0].name, Name::bare("rule"));
            let text: String = first_para(&items[0].content)
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
fn a_trailing_connect_may_hold_a_pipe_run_on_the_same_line() {
    // `::note| text` is `[content]` spelled without brackets: the connect `note` holds `text`, whether
    // the item's own text is bracket-less (`- item`) or the connect follows a group.
    for src in [
        "- item ::note| the note text\n",
        "- item ::rule(allow: x)| the note text\n",
    ] {
        let doc = parse_file(src).unwrap();
        let Block::List(list) = &doc.body[0] else {
            panic!("expected list for {src:?}")
        };
        let item = &list.items[0];
        assert_eq!(item.connects.len(), 1, "{src:?}");
        assert!(item.connects[0].content.is_some(), "{src:?}");
        let text: String = first_para(&item.content)
            .iter()
            .map(|inline| match inline {
                Inline::Text(t) => t.value.as_str(),
                other => panic!("expected text, got {other:?}"),
            })
            .collect();
        assert_eq!(text, "item", "{src:?}");
    }
}

#[test]
fn mixed_list_markers_split_into_separate_lists() {
    let doc = parse_file("- one\n-. two\n").unwrap();
    assert_eq!(doc.body.len(), 2);
    match (&doc.body[0], &doc.body[1]) {
        (Block::List(l1), Block::List(l2)) if !l1.ordered && l2.ordered => {}
        other => panic!("expected two separate lists, got {other:?}"),
    }
}

#[test]
fn combine_notation_reads_a_named_item_with_content_and_connects() {
    // `-@name(...)` -- the combine notation. `item.element` ends up a
    // real, named `Element`, exactly like a standalone `@task(...)`
    // would produce, with its own `[content]`, bare `::(){}` merge, and
    // named `::rule(...)` connect all landing on it directly -- no
    // marker, no second grammar.
    let doc = parse_file("-@task()[content]::rule(allow:(card))\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items.len(), 1);
            let item = &items[0];
            assert_eq!(item.name, Some(Name::bare("task")));
            assert_eq!(
                item.content,
                wrap(vec![Inline::Text("content".into())]).unwrap()
            );
            assert_eq!(item.connects.len(), 1);
            assert_eq!(item.connects[0].name, Name::bare("rule"));
            assert_eq!(items[0].sublist, None);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn combine_notation_supports_a_bare_trailing_connect() {
    // `::(){}` merges into the combine item's own `args`/`value`, the same
    // slot a standalone `@task()::(x: 1){y: 2}` would use.
    let doc = parse_file("-@task()::(x: 1){y: 2}\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let item = &list.items[0];
            assert_eq!(
                item.data.args,
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
    let combine_doc = parse_file("-@task()\n").unwrap();
    let Block::List(list) = &combine_doc.body[0] else {
        panic!("expected list");
    };
    let combine_el = list.items[0].as_element_view().expect("a named item");

    let standalone_doc = parse_file("@task()\n").unwrap();
    let Block::Element(standalone_el) = &standalone_doc.body[0] else {
        panic!("expected element");
    };

    assert_eq!(combine_el.name, &standalone_el.name);
    assert_eq!(combine_el.placement, standalone_el.placement);
    assert_eq!(combine_el.data, &standalone_el.data);
}

#[test]
fn a_nested_list_becomes_the_items_own_sublist() {
    // `- a\n  - b\n` -- the indented `- b` is `a`'s own `sublist`
    // (`Option<List>`), not a block inside `a`'s own `content`.
    let doc = parse_file("- a\n  - b\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            assert_eq!(list.items.len(), 1);
            let item = &list.items[0];
            assert_eq!(item.content, wrap(vec![Inline::Text("a".into())]).unwrap());
            let sub = item.sublist.as_ref().expect("nested sub-list");
            assert_eq!(sub.items.len(), 1);
            assert_eq!(
                sub.items[0].content,
                wrap(vec![Inline::Text("b".into())]).unwrap()
            );
            assert_eq!(sub.items[0].sublist, None);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn multiple_levels_of_nesting_still_work() {
    let doc = parse_file("- a\n  - b\n    - c\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let a = &list.items[0];
            assert_eq!(a.content, wrap(vec![Inline::Text("a".into())]).unwrap());
            let level_b = a.sublist.as_ref().expect("level b sub-list");
            let b = &level_b.items[0];
            assert_eq!(b.content, wrap(vec![Inline::Text("b".into())]).unwrap());
            let level_c = b.sublist.as_ref().expect("level c sub-list");
            let c = &level_c.items[0];
            assert_eq!(c.content, wrap(vec![Inline::Text("c".into())]).unwrap());
            assert_eq!(c.sublist, None);
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn a_continuation_line_that_opens_with_a_paren_is_prose_not_the_items_args() {
    // Only a `:` on the next line hands a group over to the item; a parenthetical
    // that happens to start the line is text.
    let doc = parse_file("- a b\n(c). d\n").unwrap();
    let Block::List(list) = &doc.body[0] else {
        panic!()
    };
    assert_eq!(list.items[0].data.args, None);
}

#[test]
fn a_brace_value_with_a_bare_value_in_it_is_text_at_the_end_of_an_item() {
    // `{x}` would be an error as a group (a `{...}` holds `key: value` entries),
    // so it is not the item's trailing group and stays in the text.
    let doc = parse_file("- set {x}\n").unwrap();
    let Block::List(list) = &doc.body[0] else {
        panic!()
    };
    assert_eq!(list.items[0].data.value, None);
}
