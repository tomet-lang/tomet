use tomet_ast::{Block, Document, Element, Value};
use tomet_tree::ValueExt;

use crate::{ElementKind, classify_lenient};

/// Returns this document's `@meta` element's parsed data, if it has one --
/// the `{...}` group from `@meta(format:...){...}`, already normalized to
/// a `Value` regardless of which embedded format (JSON/YAML/TOML/the
/// default DSL) it was written in. Callers pull whatever key they care
/// about (`title`, `date`, `tags`, ...) out of the returned map themselves.
///
/// `None` if the document has no `@meta` element, or that element has no
/// `{value}` group at all. Only the first top-level `@meta` is considered
/// -- `@meta` is documented as `singleton: true` (see
/// `docs/spec/builtin-settings.tmt`), so a well-formed document
/// never has more than one anyway.
pub fn document_meta(doc: &Document) -> Option<Value> {
    doc.blocks.iter().find_map(|block| match block {
        Block::Element(el) if classify_lenient(el) == ElementKind::Meta => meta_data(el),
        _ => None,
    })
}

fn meta_data(el: &Element) -> Option<Value> {
    el.value.as_ref().and_then(|v| v.as_data())
}

use crate::positional::normalized_element_args;

/// Returns this document's declared kind string (e.g. `"j.daily"`, `"config"`, `"image_note"`),
/// resolved strictly from top-level `@kind(...)` / `<kind>(...)` elements.
pub fn document_kind(doc: &Document) -> Option<String> {
    for block in &doc.blocks {
        if let Block::Element(el) = block {
            if classify_lenient(el) == ElementKind::Kind {
                if let Some(val) = normalized_element_args(el) {
                    if let Some(s) = val.get("kind").and_then(|v| v.as_str()) {
                        return Some(s.to_string());
                    }
                    if let Some(s) = val.as_str() {
                        return Some(s.to_string());
                    }
                }
                if let Some(val) = el.value.as_ref().and_then(|v| v.as_data()) {
                    if let Some(s) = val.get("kind").and_then(|v| v.as_str()) {
                        return Some(s.to_string());
                    }
                    if let Some(s) = val.as_str() {
                        return Some(s.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Returns this document's declared language/syntax version (e.g. `"1.0"`),
/// resolved strictly from top-level `@version(...)` / `<version>(...)` elements.
pub fn document_version(doc: &Document) -> Option<String> {
    for block in &doc.blocks {
        if let Block::Element(el) = block {
            if classify_lenient(el) == ElementKind::Version {
                if let Some(val) = normalized_element_args(el) {
                    if let Some(v) = val.get("version") {
                        return Some(value_to_version_string(v));
                    }
                    return Some(value_to_version_string(&val));
                }
                if let Some(val) = el.value.as_ref().and_then(|v| v.as_data()) {
                    if let Some(v) = val.get("version") {
                        return Some(value_to_version_string(v));
                    }
                    return Some(value_to_version_string(&val));
                }
            }
        }
    }

    None
}

fn value_to_version_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        _ => format!("{v:?}"),
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

    #[test]
    fn extracts_document_kind_from_kind_directive() {
        let doc = parse_document("@kind(j.daily)\n\n#[ Daily Note ]\n").unwrap();
        assert_eq!(document_kind(&doc), Some("j.daily".to_string()));

        let doc2 = parse_document("<kind>(config)\n").unwrap();
        assert_eq!(document_kind(&doc2), Some("config".to_string()));

        // No @kind directive -> returns None
        let doc3 = parse_document("@meta{\n  title: Test\n}\n").unwrap();
        assert_eq!(document_kind(&doc3), None);
    }

    #[test]
    fn extracts_document_version_from_version_directive() {
        let doc = parse_document("@version(1.0)\n\n#[ Doc ]\n").unwrap();
        assert_eq!(document_version(&doc), Some("1".to_string()));

        let doc2 = parse_document("@version(\"1.0\")\n").unwrap();
        assert_eq!(document_version(&doc2), Some("1.0".to_string()));

        // No @version directive -> returns None
        let doc3 = parse_document("@meta{\n  title: Test\n}\n").unwrap();
        assert_eq!(document_version(&doc3), None);
    }
}
