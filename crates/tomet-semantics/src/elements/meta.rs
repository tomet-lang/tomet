use tomet_ast::{Document, Element, Value};
use tomet_tree::ValueExt;

use crate::{ElementKind, classify_std_lenient};

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
///
/// "Top-level" is [`tomet_tree::for_each_top_level_element`], not a raw
/// `doc.blocks` walk: `@meta` still counts as top-level after joining an
/// adjacent paragraph (`docs/spec/syntax.tmt`'s `##[ 区切り ]`), a
/// tree-shape choice unrelated to whether it opened its own line.
pub fn document_meta(doc: &Document) -> Option<Value> {
    let mut found = None;
    tomet_tree::for_each_top_level_element(doc, |el| {
        if found.is_none() && classify_std_lenient(el) == ElementKind::Meta {
            found = meta_data(el);
        }
    });
    found
}

fn meta_data(el: &Element) -> Option<Value> {
    crate::embedded::element_data(el)
}

use crate::positional::normalized_element_args;

/// Returns this document's declared kind string (e.g. `"j.daily"`, `"config"`, `"image_note"`),
/// resolved strictly from top-level `@kind(...)` / `<kind>(...)` elements.
pub fn document_kind(doc: &Document) -> Option<String> {
    let mut found = None;
    tomet_tree::for_each_top_level_element(doc, |el| {
        if found.is_some() || classify_std_lenient(el) != ElementKind::Kind {
            return;
        }
        if let Some(val) = normalized_element_args(el) {
            if let Some(s) = val.get("kind").and_then(|v| v.as_str()) {
                found = Some(s.to_string());
                return;
            }
            if let Some(s) = val.as_str() {
                found = Some(s.to_string());
                return;
            }
        }
        if let Some(val) = crate::embedded::element_data(el) {
            if let Some(s) = val.get("kind").and_then(|v| v.as_str()) {
                found = Some(s.to_string());
                return;
            }
            if let Some(s) = val.as_str() {
                found = Some(s.to_string());
            }
        }
    });
    found
}

/// Returns this document's declared language/syntax version (e.g. `"1.0"`),
/// resolved strictly from top-level `@version(...)` / `<version>(...)` elements.
pub fn document_version(doc: &Document) -> Option<String> {
    let mut found = None;
    tomet_tree::for_each_top_level_element(doc, |el| {
        if found.is_some() || classify_std_lenient(el) != ElementKind::Version {
            return;
        }
        if let Some(val) = normalized_element_args(el) {
            found = Some(match val.get("version") {
                Some(v) => value_to_version_string(v),
                None => value_to_version_string(&val),
            });
            return;
        }
        if let Some(val) = crate::embedded::element_data(el) {
            found = Some(match val.get("version") {
                Some(v) => value_to_version_string(v),
                None => value_to_version_string(&val),
            });
        }
    });
    found
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
        let doc = parse_document("@meta(format:yaml)+++\ntitle: Hello\n+++\n").unwrap();
        let value = document_meta(&doc).expect("expected #meta value");
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

        let doc2 = parse_document("@kind(config)\n").unwrap();
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

    #[test]
    fn normalizes_list_call_in_meta_value_to_seq() {
        let doc = parse_document("@meta{\n  tags: list(rust, tomet)\n}\n").unwrap();
        let value = document_meta(&doc).expect("expected @meta value");
        match value {
            Value::Map(entries) => {
                let tags = entries.iter().find(|(k, _)| k == "tags").map(|(_, v)| v);
                assert_eq!(
                    tags,
                    Some(&Value::Seq(vec![
                        Value::String("rust".to_string()),
                        Value::String("tomet".to_string())
                    ]))
                );
            }
            other => panic!("expected a map, got {other:?}"),
        }
    }
}
