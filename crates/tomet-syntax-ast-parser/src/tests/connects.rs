use super::*;

#[test]
fn section_supports_inline_colon_connection() {
    let doc = parse_file("=[ Overview ]::{ id: intro, tag: main }\n").unwrap();
    match &doc.body[0] {
        Block::Section(s) => {
            assert_eq!(
                s.data.value,
                Some(Group::from_map(Value::Map(vec![
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
    let doc = parse_file("@task[ Task A ]::{ id: taskA, priority: high }\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("task"));
            assert_eq!(
                el.data.value,
                Some(Group::from_map(Value::Map(vec![
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
    let doc = parse_file("@id(taskA)::{ priority: high, tag: dev }\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("id"));
            assert_eq!(el.data.args, Some(Value::String("taskA".into())));
            assert_eq!(
                el.data.value,
                Some(Group::from_map(Value::Map(vec![
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
    let doc = parse_file("@section[ x ]::rule(allow: (card))\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("section"));
            assert_eq!(el.connects.len(), 1);
            let rule = &el.connects[0];
            assert_eq!(rule.name, Name::bare("rule"));
            assert_eq!(
                rule.data.args,
                Some(Value::Map(vec![(
                    "allow".into(),
                    Value::Seq(vec![Value::String("card".into())])
                )]))
            );
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn multiple_named_connects_stack_flat_not_nested() {
    let doc = parse_file("@x(a: 1)::as(y)::rule(allow: (card))\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.connects.len(), 2);
            assert_eq!(el.connects[0].name, Name::bare("as"));
            assert_eq!(el.connects[1].name, Name::bare("rule"));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn bare_colon_merge_is_unaffected_by_named_connect_support() {
    // No name after the colon -- must still take the old bare-merge
    // path, not be misread as a nameless connect.
    let doc = parse_file("@memo(a:1)::{b:2}\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert!(el.connects.is_empty());
            assert_eq!(
                el.data.value,
                Some(Group::from_map(Value::Map(vec![(
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
    let doc = parse_file("=[ h ]::rule(allow: (card))\n").unwrap();
    match &doc.body[0] {
        Block::Section(s) => {
            assert_eq!(s.connects.len(), 1);
            assert_eq!(s.connects[0].name, Name::bare("rule"));
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn sugar_list_item_and_heading_support_a_bare_connect() {
    // `::()`, `::{}` and the combined `::(){}` now attach to a bracket-less
    // sugar body's own owner the same way they already did for a
    // bracketed one -- see `ConnectMode::Bare`/`find_trailing_connect`.
    for (src, expect_args, expect_value) in [
        ("- content ::(x: 1)\n", Some(Value::Int(1)), None),
        ("- content ::{y: 2}\n", None, Some(Value::Int(2))),
        (
            "- content ::(x: 1){y: 2}\n",
            Some(Value::Int(1)),
            Some(Value::Int(2)),
        ),
    ] {
        let doc = parse_file(src).unwrap_or_else(|e| panic!("{src:?} failed: {e}"));
        match &doc.body[0] {
            Block::List(list) => {
                let items = &list.items;
                assert_eq!(
                    items[0].data.args,
                    expect_args.map(|v| Value::Map(vec![("x".into(), v)])),
                    "{src:?}"
                );
                assert_eq!(
                    item_attrs(&items[0]),
                    expect_value.map(|v| Value::Map(vec![("y".into(), v)])),
                    "{src:?}"
                );
                let text: String = first_para(&items[0].content)
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

    let doc = parse_file("= title ::(x: 1){y: 2}\n").unwrap();
    match &doc.body[0] {
        Block::Section(s) => {
            assert_eq!(
                s.data.args,
                Some(Value::Map(vec![("x".into(), Value::Int(1))]))
            );
            assert_eq!(
                s.data.value,
                Some(Group::from_map(Value::Map(vec![(
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
    // has always supported `::name(...)`; the sugar form didn't, purely
    // because `parse_sugar_body` had no colon handling of any kind
    // before -- `ConnectMode::Full` closes that gap rather than leaving
    // the sugar form one step behind the bracketed one.
    let doc = parse_file("= h ::rule(allow: (card))\n").unwrap();
    match &doc.body[0] {
        Block::Section(s) => {
            assert_eq!(s.connects.len(), 1);
            assert_eq!(s.connects[0].name, Name::bare("rule"));
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn sugar_list_item_supports_a_named_connect_matching_its_bracketed_form() {
    // A list item's sugar body now supports `::name(...)` too -- matching
    // both its own full-bracketed form (just above) and a heading's sugar
    // form, now that `element_list_item` carries `connects`.
    let doc = parse_file("- content ::rule(allow: (card))\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(items[0].connects.len(), 1);
            assert_eq!(items[0].connects[0].name, Name::bare("rule"));
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
    let doc = parse_file("- content\n::{y: 2}\n").unwrap();
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
    assert_eq!(doc.body.len(), 1, "no stray paragraph split off");

    let doc = parse_file("- content\n\n::{y: 2}\n").unwrap();
    match &doc.body[0] {
        Block::List(list) => {
            let items = &list.items;
            assert_eq!(item_attrs(&items[0]), None);
        }
        other => panic!("expected list, got {other:?}"),
    }
    assert_eq!(
        doc.body.len(),
        2,
        "two blank lines end the item before the connect"
    );

    let doc = parse_file("= title\n::{y: 2}\n").unwrap();
    match &doc.body[0] {
        Block::Section(s) => {
            assert_eq!(
                s.data.value,
                Some(Group::from_map(Value::Map(vec![(
                    "y".into(),
                    Value::Int(2)
                )])))
            );
        }
        other => panic!("expected section, got {other:?}"),
    }
}

// --- `::` is the connect sigil; a single `:` is only `key: value` ----------------

/// The hosts of a postfix connect: an element, a section, a list item, each written the
/// bracketed way and the bracket-less way.
#[test]
fn a_double_colon_connect_attaches_to_every_host() {
    for src in [
        "@el(a)::rule(allow: (card))\n",
        "@el(a) ::rule(allow: (card))\n",
        "=[ H ]::rule(allow: (card))\n",
        "= H ::rule(allow: (card))\n",
        "- [ x ]::rule(allow: (card))\n",
        "- item ::rule(allow: (card))\n",
    ] {
        let doc = parse_file(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
        let connects = match &doc.body[0] {
            Block::Element(el) => el.connects.clone(),
            Block::Section(s) => s.connects.clone(),
            Block::List(l) => l.items[0].connects.clone(),
            other => panic!("{src:?}: {other:?}"),
        };
        assert_eq!(connects.len(), 1, "{src:?}");
        assert_eq!(connects[0].name, Name::bare("rule"), "{src:?}");
        assert_eq!(
            connects[0].data.args,
            Some(Value::Map(vec![(
                "allow".into(),
                Value::Seq(vec![Value::String("card".into())])
            )])),
            "{src:?}"
        );
    }
}

#[test]
fn a_nameless_double_colon_connect_merges_into_its_owner() {
    let doc = parse_file("=[ H ]::{ a: 1 }\n").unwrap();
    let Block::Section(s) = &doc.body[0] else {
        panic!("expected a section")
    };
    assert_eq!(
        s.data.value,
        Some(Group::from_map(Value::Map(vec![(
            "a".into(),
            Value::Int(1)
        )])))
    );
    assert!(s.connects.is_empty());
}

/// `::id(x)` is the `#(x)` of its owner and `::meta(a: 1)` its `{a: 1}`, written out.
#[test]
fn id_and_meta_connects_are_the_sugars_written_out() {
    let sugar = parse_file("@el(a: 1)#(x){ k: v }\n").unwrap();
    let written = parse_file("@el(a: 1)::id(x)::meta(k: v)\n").unwrap();
    let (Block::Element(s), Block::Element(w)) = (&sugar.body[0], &written.body[0]) else {
        panic!("expected elements")
    };
    assert_eq!(w.id, s.id);
    assert_eq!(w.data, s.data);
    assert!(w.connects.is_empty());

    for (src, message) in [
        ("@el()#(a)::id(b)\n", "already written"),
        ("@el()::id()\n", "one id"),
        ("@el()::id(a, b)\n", "one id"),
        ("@el()::meta(1)\n", "`key: value` pairs"),
        ("@el()::id[x]\n", "takes its `(...)` only"),
    ] {
        let err = parse_file(src).unwrap_err();
        assert!(err.message.contains(message), "{src:?}: {}", err.message);
    }
}

#[test]
fn a_single_colon_connect_is_refused_and_says_what_to_write() {
    for (src, hint) in [
        ("@el(a):rule(b)\n", "`::rule(...)`, not `:rule(...)`"),
        ("@el(a):{b: 1}\n", "`::{...}`"),
        ("@el(a):(b: 1)\n", "`::{...}`"),
        ("=[ H ]:rule(b)\n", "`::rule(...)`"),
        ("= H :rule(b)\n", "`::rule(...)`"),
        ("- item :{ a: 1 }\n", "`::{...}`"),
        ("- [ x ]:rule(b)\n", "`::rule(...)`"),
        ("@el(a)\n:rule(b)\n", "`::rule(...)`"),
    ] {
        let err = parse_file(src).unwrap_err();
        assert!(err.message.contains(hint), "{src:?}: {}", err.message);
        assert!(
            err.message.contains("`:` only separates `key: value`"),
            "{src:?}: {}",
            err.message
        );
    }
}

/// A `::name` at the start of a line is a declaration, never the connect of the line above.
#[test]
fn a_double_colon_name_on_the_next_line_is_not_a_connect() {
    let doc = parse_file("::kind(config)\n::rule(allow: (card))\n").unwrap();
    assert_eq!(doc.header.len(), 2);
    let err = parse_file("@el(a)\n::rule(b)\n").unwrap_err();
    assert!(
        err.message.contains("declaration belongs at the top"),
        "{}",
        err.message
    );
}
