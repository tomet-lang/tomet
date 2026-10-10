use super::*;

#[test]
fn section_supports_inline_colon_connection() {
    let doc = parse_document("=[ Overview ]:{ id: intro, tag: main }\n").unwrap();
    match &doc.blocks[0] {
        Block::Section(s) => {
            assert_eq!(
                s.value,
                Some(ElementValue::from_map(Value::Map(vec![
                    ("id".into(), Value::String("intro".into())),
                    ("tag".into(), Value::String("main".into())),
                ])))
            );
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn element_supports_inline_colon_connection() {
    let doc = parse_document("@task[ Task A ]:{ id: taskA, priority: high }\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("task"));
            assert_eq!(
                el.value,
                Some(ElementValue::from_map(Value::Map(vec![
                    ("id".into(), Value::String("taskA".into())),
                    ("priority".into(), Value::String("high".into())),
                ])))
            );
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn remote_id_target_element_supports_colon_connection() {
    let doc = parse_document("@id(taskA):{ priority: high, tag: dev }\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("id"));
            assert_eq!(el.args, Some(Value::String("taskA".into())));
            assert_eq!(
                el.value,
                Some(ElementValue::from_map(Value::Map(vec![
                    ("priority".into(), Value::String("high".into())),
                    ("tag".into(), Value::String("dev".into())),
                ])))
            );
        }
        other => panic!("expected remote id element, got {other:?}"),
    }
}

#[test]
fn named_connect_reads_into_connects() {
    let doc = parse_document("@section[ x ]:rule(allow: list(card))\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("section"));
            assert_eq!(el.connects.len(), 1);
            let rule = &el.connects[0];
            assert_eq!(rule.sigil, Sigil::named("rule"));
            assert_eq!(
                rule.args,
                Some(Value::Map(vec![(
                    "allow".into(),
                    Value::Call("list".into(), vec![Value::String("card".into())])
                )]))
            );
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn multiple_named_connects_stack_flat_not_nested() {
    let doc = parse_document("@x(a: 1):as(y):rule(allow: list(card))\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.connects.len(), 2);
            assert_eq!(el.connects[0].sigil, Sigil::named("as"));
            assert!(el.connects[0].connects.is_empty());
            assert_eq!(el.connects[1].sigil, Sigil::named("rule"));
            assert!(el.connects[1].connects.is_empty());
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn bare_colon_merge_is_unaffected_by_named_connect_support() {
    // No name after the colon -- must still take the old bare-merge
    // path, not be misread as a nameless connect.
    let doc = parse_document("@memo(a:1):{b:2}\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert!(el.connects.is_empty());
            assert_eq!(
                el.value,
                Some(ElementValue::from_map(Value::Map(vec![(
                    "b".into(),
                    Value::Int(2)
                )])))
            );
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn section_supports_named_connect() {
    let doc = parse_document("=[ h ]:rule(allow: list(card))\n").unwrap();
    match &doc.blocks[0] {
        Block::Section(s) => {
            assert_eq!(s.connects.len(), 1);
            assert_eq!(s.connects[0].sigil, Sigil::named("rule"));
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn sugar_list_item_and_heading_support_a_bare_connect() {
    // `:()`, `:{}` and the combined `:(){}` now attach to a bracket-less
    // sugar body's own owner the same way they already did for a
    // bracketed one -- see `ConnectMode::Bare`/`find_trailing_connect`.
    for (src, expect_args, expect_value) in [
        ("- content :(x: 1)\n", Some(Value::Int(1)), None),
        ("- content :{y: 2}\n", None, Some(Value::Int(2))),
        (
            "- content :(x: 1){y: 2}\n",
            Some(Value::Int(1)),
            Some(Value::Int(2)),
        ),
    ] {
        let doc = parse_document(src).unwrap_or_else(|e| panic!("{src:?} failed: {e}"));
        match &doc.blocks[0] {
            Block::List(list) => {
                let items = &list.items;
                assert_eq!(
                    items[0].element.args,
                    expect_args.map(|v| Value::Map(vec![("x".into(), v)])),
                    "{src:?}"
                );
                assert_eq!(
                    item_attrs(&items[0].element),
                    expect_value.map(|v| Value::Map(vec![("y".into(), v)])),
                    "{src:?}"
                );
                let text: String = first_para(items[0].element.content.as_ref().expect("content"))
                    .iter()
                    .map(|inline| match inline {
                        Inline::Text(t) => t.value.as_str(),
                        other => panic!("{src:?}: expected text, got {other:?}"),
                    })
                    .collect();
                assert_eq!(text, "content", "{src:?}");
            }
            other => panic!("{src:?}: expected list, got {other:?}"),
        }
    }

    let doc = parse_document("= title :(x: 1){y: 2}\n").unwrap();
    match &doc.blocks[0] {
        Block::Section(s) => {
            assert_eq!(s.args, Some(Value::Map(vec![("x".into(), Value::Int(1))])));
            assert_eq!(
                s.value,
                Some(ElementValue::from_map(Value::Map(vec![(
                    "y".into(),
                    Value::Int(2)
                )])))
            );
            assert_eq!(s.title, vec![Inline::Text("title".into())]);
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn sugar_heading_supports_a_named_connect_matching_its_bracketed_form() {
    // A bracketed heading (`section_supports_named_connect`, just above)
    // has always supported `:name(...)`; the sugar form didn't, purely
    // because `parse_sugar_body` had no colon handling of any kind
    // before -- `ConnectMode::Full` closes that gap rather than leaving
    // the sugar form one step behind the bracketed one.
    let doc = parse_document("= h :rule(allow: list(card))\n").unwrap();
    match &doc.blocks[0] {
        Block::Section(s) => {
            assert_eq!(s.connects.len(), 1);
            assert_eq!(s.connects[0].sigil, Sigil::named("rule"));
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn sugar_list_item_supports_a_named_connect_matching_its_bracketed_form() {
    // A list item's sugar body now supports `:name(...)` too -- matching
    // both its own full-bracketed form (just above) and a heading's sugar
    // form, now that `element_list_item` carries `connects`.
    let doc = parse_document("- content :rule(allow: list(card))\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items[0].element.connects.len(), 1);
            assert_eq!(items[0].element.connects[0].sigil, Sigil::named("rule"));
        }
        other => panic!("expected list, got {other:?}"),
    }
}

#[test]
fn sugar_connect_tolerates_one_blank_line_but_not_two() {
    // The same gap `skip_element_gap` already grants a bracketed
    // element's own groups: a trailing connect may start on the very
    // next line, or after exactly one blank line, but a second blank
    // line ends the block before the connect is ever reached.
    let doc = parse_document("- content\n:{y: 2}\n").unwrap();
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
    assert_eq!(doc.blocks.len(), 1, "no stray paragraph split off");

    let doc = parse_document("- content\n\n:{y: 2}\n").unwrap();
    match &doc.blocks[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(item_attrs(&items[0].element), None);
        }
        other => panic!("expected list, got {other:?}"),
    }
    assert_eq!(
        doc.blocks.len(),
        2,
        "two blank lines end the item before the connect"
    );

    let doc = parse_document("= title\n:{y: 2}\n").unwrap();
    match &doc.blocks[0] {
        Block::Section(s) => {
            assert_eq!(
                s.value,
                Some(ElementValue::from_map(Value::Map(vec![(
                    "y".into(),
                    Value::Int(2)
                )])))
            );
        }
        other => panic!("expected section, got {other:?}"),
    }
}
