use tomet_ast::{Block, Document, Element, ElementValue, Value};

use crate::{ElementKind, classify};

/// Returns this document's `@meta` element's parsed data, if it has one --
/// the `{...}` group from `@meta(format:...){...}`, already normalized to
/// a `Value` regardless of which embedded format (JSON/YAML/TOML/the
/// default DSL) it was written in. Callers pull whatever key they care
/// about (`title`, `date`, `tags`, ...) out of the returned map themselves.
///
/// `None` if the document has no `@meta` element, or that element has no
/// `{value}` group at all. Only the first top-level `@meta` is considered
/// -- `@meta` is documented as `singleton: true` (see
/// `docs/ja/specifications/builtin.settings.tmt`), so a well-formed document
/// never has more than one anyway.
pub fn document_meta(doc: &Document) -> Option<&Value> {
    doc.blocks.iter().find_map(|block| match block {
        Block::Element(el) if classify(el) == ElementKind::Meta => meta_data(el),
        _ => None,
    })
}

fn meta_data(el: &Element) -> Option<&Value> {
    match &el.value {
        Some(ElementValue::Data(v)) => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_parser::parse_document;

    #[test]
    fn finds_meta_value_from_a_meta_element() {
        let doc = parse_document("@meta(format:yaml){\n  title: Hello\n}\n").unwrap();
        let value = document_meta(&doc).expect("expected @meta value");
        match value {
            Value::Map(map) => {
                assert_eq!(
                    map.iter().find(|(k, _)| k == "title"),
                    Some(&("title".to_string(), Value::String("Hello".to_string())))
                );
            }
            other => panic!("expected a map, got {other:?}"),
        }
    }

    #[test]
    fn no_meta_element_is_none() {
        let doc = parse_document("#[ Hello ]\n").unwrap();
        assert_eq!(document_meta(&doc), None);
    }

    #[test]
    fn meta_element_without_a_value_group_is_none() {
        let doc = parse_document("@meta(format:yaml)\n").unwrap();
        assert_eq!(document_meta(&doc), None);
    }
}
