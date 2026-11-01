use super::*;
use tomet_ast::Entry;

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
fn parses_links_container_with_bare_children() {
    let doc = parse_file("@links {\n  (1)[ note ]\n  (anotation1)[ note ]\n}\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("links"));
            match &el.data.value {
                Some(Group(entries)) => {
                    let [Entry::Bare(first), Entry::Bare(second)] = entries.as_slice() else {
                        panic!("expected two bare entries, got {entries:?}");
                    };
                    assert_eq!(first.data.args, Some(Value::Int(1)));
                    assert_eq!(first.content, wrap(vec![Inline::Text("note".into())]));
                    assert_eq!(second.data.args, Some(Value::String("anotation1".into())));
                }
                other => panic!("expected children, got {other:?}"),
            }
        }
        other => panic!("expected element, got {other:?}"),
    }
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
fn a_bare_entry_takes_args_content_and_value_like_a_named_one() {
    let doc = parse_file("@memo{ (1)[ ひとつ ]{ id: a } }\n").unwrap();
    let Block::Element(el) = &doc.body[0] else {
        panic!()
    };
    let Some(Group(entries)) = &el.data.value else {
        panic!()
    };
    let [Entry::Bare(bare)] = entries.as_slice() else {
        panic!("expected one bare entry, got {entries:?}");
    };
    assert_eq!(bare.data.args, Some(Value::Int(1)));
    assert!(bare.content.is_some());
    assert!(bare.data.value.is_some());
}
