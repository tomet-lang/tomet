mod document;
mod error;
mod value;

pub use document::parse_document;
pub use error::{Error, Result};
pub use value::parse_value;

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_ast::{Block, ElementValue, Inline, Sigil, Value};

    #[test]
    fn parses_flat_map() {
        let v = parse_value("title: value\ntags: [a, b]").unwrap();
        assert_eq!(
            v,
            Value::Map(vec![
                ("title".into(), Value::String("value".into())),
                ("tags".into(), Value::Seq(vec![Value::String("a".into()), Value::String("b".into())])),
            ])
        );
    }

    #[test]
    fn parses_scalars() {
        assert_eq!(parse_value("42").unwrap(), Value::Int(42));
        assert_eq!(parse_value("-3.5").unwrap(), Value::Float(-3.5));
        assert_eq!(parse_value("true").unwrap(), Value::Bool(true));
        assert_eq!(parse_value("null").unwrap(), Value::Null);
        assert_eq!(parse_value("hello").unwrap(), Value::String("hello".into()));
    }

    #[test]
    fn preserves_colon_in_url_values() {
        let v = parse_value("url: https://example.com/path").unwrap();
        assert_eq!(
            v,
            Value::Map(vec![("url".into(), Value::String("https://example.com/path".into()))])
        );
    }

    #[test]
    fn parses_heading_with_attrs() {
        let doc = parse_document("#[ Hello ]{ id:header1 }\n").unwrap();
        match &doc.blocks[0] {
            Block::Heading(h) => {
                assert_eq!(h.level, 1);
                assert_eq!(h.content, vec![Inline::Text("Hello".into())]);
                assert_eq!(h.attrs, Some(Value::Map(vec![("id".into(), Value::String("header1".into()))])));
            }
            other => panic!("expected heading, got {other:?}"),
        }
    }

    #[test]
    fn parses_link_with_order_free_groups() {
        let a = parse_document("@(url:https://example.com)[Wiki]").unwrap();
        let b = parse_document("@[Wiki](url:https://example.com)").unwrap();
        assert_eq!(a, b);
        match &a.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::At(None));
                assert_eq!(
                    el.input,
                    Some(Value::Map(vec![("url".into(), Value::String("https://example.com".into()))]))
                );
                assert_eq!(el.area, Some(vec![Inline::Text("Wiki".into())]));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parses_links_container_with_bare_children() {
        let doc = parse_document("@links {\n  (1)[ note ]\n  (anotation1)[ note ]\n}\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::At(Some("links".into())));
                match &el.value {
                    Some(ElementValue::Children(children)) => {
                        assert_eq!(children.len(), 2);
                        assert_eq!(children[0].sigil, Sigil::Bare);
                        assert_eq!(children[0].input, Some(Value::Int(1)));
                        assert_eq!(children[0].area, Some(vec![Inline::Text("note".into())]));
                        assert_eq!(children[1].input, Some(Value::String("anotation1".into())));
                    }
                    other => panic!("expected children, got {other:?}"),
                }
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parses_caution_as_typed_element_not_bare_bracket() {
        let doc = parse_document("<caution>[ be careful ]\n").unwrap();
        match &doc.blocks[0] {
            Block::Element(el) => {
                assert_eq!(el.sigil, Sigil::Type("caution".into()));
                assert_eq!(el.area, Some(vec![Inline::Text("be careful".into())]));
            }
            other => panic!("expected element, got {other:?}"),
        }
    }

    #[test]
    fn parses_list() {
        let doc = parse_document("- one\n- two\n").unwrap();
        match &doc.blocks[0] {
            Block::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].content, vec![Inline::Text("one".into())]);
                assert_eq!(items[1].content, vec![Inline::Text("two".into())]);
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn parses_the_repo_spec_examples() {
        parse_document(include_str!("../../../docs/tmt/typedmark.tm")).unwrap();
        parse_document(include_str!("../../../docs/tmt/image_meta.tm")).unwrap();
    }

    #[test]
    fn bare_at_is_plain_text_when_not_an_element() {
        let doc = parse_document("contact me@example.com please\n").unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(inlines) => {
                assert_eq!(inlines, &vec![Inline::Text("contact me@example.com please".into())]);
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }
}
