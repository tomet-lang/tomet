use super::*;
use tomet_ast::Entry;

#[test]
fn parses_section_with_attrs() {
    let doc = parse_document("=[ Hello ]{ id:header1 }\n").unwrap();
    match &doc.blocks[0] {
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
    let a = parse_document("@link(target:https://example.com)[Wiki]").unwrap();
    let b = parse_document("@link[Wiki](target:https://example.com)").unwrap();
    assert_eq!(a, b);
    match &a.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("link"));
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
    let doc = parse_document("@links {\n  (1)[ note ]\n  (anotation1)[ note ]\n}\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("links"));
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
    let doc = parse_document("@caution[ be careful ]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("caution"));
            assert_eq!(el.content, wrap(vec![Inline::Text("be careful".into())]));
        }
        other => panic!("expected element, got {other:?}"),
    }
}
