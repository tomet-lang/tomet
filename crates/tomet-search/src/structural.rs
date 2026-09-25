//! Structural AST query matching.
//!
//! Provides pure in-memory querying over Tomet AST elements by tag,
//! attribute/property key, and value content.

use tomet_ast::{Document, Element, ElementValue, Span, Value};
use tomet_semantics::classify_std_lenient;
use tomet_tree::{ElementExt, for_each_element};

/// Criteria for matching AST elements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructuralQuery {
    /// Tag (kind) name to match (case-insensitive, e.g. "meta", "link", "heading").
    pub tag: Option<String>,
    /// Attribute or property key that must exist on the element.
    pub key: Option<String>,
    /// Substring that must appear in the element's args or value.
    pub value_contains: Option<String>,
}

/// A matched AST element occurrence with its span and tag name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralMatch {
    pub span: Span,
    pub tag: String,
}

/// Checks if a single element matches `query`.
pub fn matches_query(el: &Element, query: &StructuralQuery) -> bool {
    let kind = classify_std_lenient(el);
    if let Some(target_tag) = &query.tag
        && !target_tag.is_empty()
        && !kind.as_str().eq_ignore_ascii_case(target_tag)
    {
        return false;
    }

    if let Some(target_key) = &query.key
        && !target_key.is_empty()
        && !el.has_prop_key(target_key)
    {
        return false;
    }

    if let Some(sub) = &query.value_contains
        && !sub.is_empty()
    {
        let in_args = el.args.as_ref().is_some_and(|v| value_contains_str(v, sub));
        let in_val = el.value.as_ref().is_some_and(|v| match v {
            ElementValue::Group(_) => v
                .as_data()
                .is_some_and(|data| value_contains_str(&data, sub)),
            ElementValue::Raw(body) => body.contains(sub),
            ElementValue::Interp(_) => false,
        });
        if !in_args && !in_val {
            return false;
        }
    }

    true
}

/// Counts matching elements in `doc` for `query`.
pub fn count_structural_matches(doc: &Document, query: &StructuralQuery) -> usize {
    let mut count = 0;
    for_each_element(doc, |el| {
        if matches_query(el, query) {
            count += 1;
        }
    });
    count
}

/// Finds all matching element occurrences in `doc` for `query`.
pub fn find_structural_matches(doc: &Document, query: &StructuralQuery) -> Vec<StructuralMatch> {
    let mut matches = Vec::new();
    for_each_element(doc, |el| {
        if matches_query(el, query) {
            let kind = classify_std_lenient(el);
            matches.push(StructuralMatch {
                span: el.span,
                tag: kind.as_str().to_string(),
            });
        }
    });
    matches
}

fn value_contains_str(v: &Value, sub: &str) -> bool {
    match v {
        Value::String(s) => s.contains(sub),
        Value::Map(entries) => entries
            .iter()
            .any(|(k, val)| k.contains(sub) || value_contains_str(val, sub)),
        Value::Seq(items) => items.iter().any(|item| value_contains_str(item, sub)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_parser::parse_document;

    #[test]
    fn matches_query_by_tag() {
        let doc = parse_document("@meta{author: \"Alice\"}\n#[ Title ]\n").unwrap();
        let query = StructuralQuery {
            tag: Some("meta".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &query), 1);
        let matches = find_structural_matches(&doc, &query);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tag, "meta");
    }

    #[test]
    fn matches_query_by_key() {
        let doc = parse_document("@meta{author: \"Alice\", draft: true}\n").unwrap();
        let q_author = StructuralQuery {
            key: Some("author".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &q_author), 1);

        let q_missing = StructuralQuery {
            key: Some("nonexistent".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &q_missing), 0);
    }

    #[test]
    fn matches_query_by_value_contains() {
        let doc = parse_document("@meta{author: \"Alice Wonderland\"}\n").unwrap();
        let q_val = StructuralQuery {
            value_contains: Some("Wonder".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &q_val), 1);

        let q_nomatch = StructuralQuery {
            value_contains: Some("Bob".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &q_nomatch), 0);
    }
}
