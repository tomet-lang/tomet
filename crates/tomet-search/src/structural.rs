//! Structural AST query matching.
//!
//! Provides pure in-memory querying over Tomet AST elements by tag,
//! attribute/property key, and value content.

use tomet_ast::{Data, Declaration, Element, File, Value};
use tomet_span::Span;
use tomet_semantics::{classify_declaration_lenient, classify_std_lenient};
use tomet_tree::{DataExt, for_each_declaration, for_each_element};

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
    matches_parts(classify_std_lenient(el.as_view()).as_str(), &el.data, query)
}

/// Checks if a single declaration matches `query`.
pub fn matches_declaration_query(declaration: &Declaration, query: &StructuralQuery) -> bool {
    matches_parts(
        classify_declaration_lenient(declaration).as_str(),
        &declaration.data,
        query,
    )
}

fn matches_parts(tag: &str, data: &Data, query: &StructuralQuery) -> bool {
    if let Some(target_tag) = &query.tag
        && !target_tag.is_empty()
        && !tag.eq_ignore_ascii_case(target_tag)
    {
        return false;
    }

    if let Some(target_key) = &query.key
        && !target_key.is_empty()
        && !data.has_prop_key(target_key)
    {
        return false;
    }

    if let Some(sub) = &query.value_contains
        && !sub.is_empty()
    {
        let in_args = data
            .args
            .as_ref()
            .is_some_and(|v| value_contains_str(v, sub));
        let in_val = data
            .value
            .as_ref()
            .is_some_and(|v| value_contains_str(&v.as_data(), sub));
        if !in_args && !in_val {
            return false;
        }
    }

    true
}

/// Counts matching elements and declarations in `doc` for `query`.
pub fn count_structural_matches(doc: &File, query: &StructuralQuery) -> usize {
    find_structural_matches(doc, query).len()
}

/// Finds all matching element and declaration occurrences in `doc` for
/// `query`, in document order.
pub fn find_structural_matches(doc: &File, query: &StructuralQuery) -> Vec<StructuralMatch> {
    let mut matches = Vec::new();
    for_each_element(doc, |el| {
        if matches_query(el, query) {
            let kind = classify_std_lenient(el.as_view());
            matches.push(StructuralMatch {
                span: el.span,
                tag: kind.as_str().to_string(),
            });
        }
    });
    for_each_declaration(doc, |declaration| {
        if matches_declaration_query(declaration, query) {
            matches.push(StructuralMatch {
                span: declaration.span,
                tag: classify_declaration_lenient(declaration)
                    .as_str()
                    .to_string(),
            });
        }
    });
    matches.sort_by_key(|m| m.span.start);
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
    use tomet_ast_parser::parse_file;

    #[test]
    fn matches_query_by_tag() {
        let doc = parse_file("::meta{author: \"Alice\"}\n#[ Title ]\n").unwrap();
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
        let doc = parse_file("::meta{author: \"Alice\", draft: true}\n").unwrap();
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
        let doc = parse_file("::meta{author: \"Alice Wonderland\"}\n").unwrap();
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
