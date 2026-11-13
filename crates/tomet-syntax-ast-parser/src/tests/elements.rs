use super::*;

#[test]
fn parses_section_with_attrs() {
    let doc = parse_file("=[ Hello ]{ id:header1 }\n").unwrap();
    match &doc.body[0] {
        Block::Section(s) => {
            assert_eq!(s.level, 1);
            assert_eq!(s.title, vec![Inline::Text("Hello".into())]);
            assert_eq!(
                s.data.value,
                Some(Group::from_map(Value::Map(vec![(
                    "id".into(),
                    Value::String("header1".into())
                )])))
            );
        }
        other => panic!("expected section, got {other:?}"),
    }
}

#[test]
fn parses_link_with_order_free_groups() {
    let a = parse_file("@link(target:https://example.com)[Wiki]").unwrap();
    let b = parse_file("@link[Wiki](target:https://example.com)").unwrap();
    assert_eq!(a, b);
    match &a.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("link"));
            assert_eq!(
                el.data.args,
                Some(Value::Map(vec![(
                    "target".into(),
                    Value::String("https://example.com".into())
                )]))
            );
            assert_eq!(el.content, wrap(vec![Inline::Text("Wiki".into())]));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn a_nameless_entry_in_a_value_group_is_rejected_with_advice() {
    let err = parse_file("@memo(){\n  (1)[ first ]\n  (2)[ second ]\n}\n").unwrap_err();
    assert_eq!(
        err.message,
        "a '{...}' group takes no nameless entry: write a named element, '@name(...)[...]', as an entry"
    );
}

#[test]
fn parses_caution_as_typed_element_not_bare_bracket() {
    let doc = parse_file("@caution[ be careful ]\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("caution"));
            assert_eq!(el.content, wrap(vec![Inline::Text("be careful".into())]));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn an_element_folded_into_a_paragraph_still_takes_its_group_from_the_next_line() {
    // The second `@file` is the second line of a paragraph, an inline element,
    // and its `[content]` stands on the line after it all the same.
    let doc = parse_file("@dir(a)\n[ x ]\n@file(b)\n[ y ]\n").unwrap();
    let [Block::Paragraph(p)] = doc.body.as_slice() else {
        panic!("expected one paragraph, got {:?}", doc.body);
    };
    let elements: Vec<&Element> = p
        .content
        .iter()
        .filter_map(|i| match i {
            Inline::Element(el) => Some(el),
            _ => None,
        })
        .collect();
    assert_eq!(elements.len(), 2);
    assert!(elements.iter().all(|el| el.content.is_some()));
}

#[test]
fn an_angle_bracket_is_a_character_not_a_typed_element() {
    let doc = parse_file("<embed>[alt](https://example.com/pic.png)\n").unwrap();
    let [Block::Paragraph(p)] = doc.body.as_slice() else {
        panic!("expected a paragraph, got {:?}", doc.body);
    };
    assert!(matches!(p.content.first(), Some(Inline::Text(t)) if t.value.starts_with("<embed>")));
}

#[test]
fn a_name_must_be_ascii_to_start_an_element() {
    let doc = parse_file("@タグ\n").unwrap();
    assert!(matches!(doc.body.as_slice(), [Block::Paragraph(_)]));
}

#[test]
fn inline_nameless_entry_in_value_group_is_rejected() {
    let err = parse_file("@memo(){ (1)[ ひとつ ]{ id: a } }\n").unwrap_err();
    assert_eq!(
        err.message,
        "a '{...}' group takes no nameless entry: write a named element, '@name(...)[...]', as an entry"
    );
}
