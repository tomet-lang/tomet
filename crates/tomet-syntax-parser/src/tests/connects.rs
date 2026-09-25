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
