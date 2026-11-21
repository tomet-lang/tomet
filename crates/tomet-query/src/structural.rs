//! Structural AST query matching.
//!
//! Provides pure in-memory querying over Tomet AST elements by tag,
//! attribute/property key, and value content.

use tomet_ast::{Data, Declaration, Element, File, Tuple, Value};
use tomet_scalar::Scalar;
use tomet_semantics::Bindings;
use tomet_span::Span;
use tomet_tree::{DataExt, for_each_declaration, for_each_element};

/// Criteria for matching AST elements.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructuralQuery {
    /// Tag (kind) name to match (case-insensitive, e.g. "meta", "link", "heading"). A name a
    /// vocabulary declares is matched in that vocabulary's own namespace (`node-graph.node`),
    /// whatever alias a document writes it under.
    pub tag: Option<String>,
    /// Attribute or property key that must exist on the element.
    pub key: Option<String>,
    /// Substring that must appear in the element's args or value.
    pub value_contains: Option<String>,
}

/// A matched AST element occurrence with its span and tag name (the kind it resolved to).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralMatch {
    pub span: Span,
    pub tag: String,
}

/// Checks if a single element matches `query`, its name resolved against `bindings` (the names
/// the element's document has in scope).
pub fn matches_query(el: &Element, bindings: &Bindings, query: &StructuralQuery) -> bool {
    matches_parts(
        bindings.classify_lenient(&el.name).as_str(),
        &el.data,
        query,
    )
}

/// Checks if a single declaration matches `query`, its name resolved against `bindings`.
pub fn matches_declaration_query(
    declaration: &Declaration,
    bindings: &Bindings,
    query: &StructuralQuery,
) -> bool {
    matches_parts(
        bindings
            .classify_attribute_lenient(&declaration.name)
            .as_str(),
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
            .is_some_and(|t| tuple_contains_str(t, sub));
        let in_val = data
            .value
            .as_ref()
            .is_some_and(|v| tuple_contains_str(&v.as_data(), sub));
        if !in_args && !in_val {
            return false;
        }
    }

    true
}

/// Counts matching elements and declarations in `doc` for `query`. `bindings` are the names
/// `doc` has in scope, as whoever loaded it bound them.
pub fn count_structural_matches(doc: &File, bindings: &Bindings, query: &StructuralQuery) -> usize {
    find_structural_matches(doc, bindings, query).len()
}

/// Finds all matching element and declaration occurrences in `doc` for
/// `query`, in document order. `bindings` as in [`count_structural_matches`].
pub fn find_structural_matches(
    doc: &File,
    bindings: &Bindings,
    query: &StructuralQuery,
) -> Vec<StructuralMatch> {
    let mut matches = Vec::new();
    for_each_element(doc, |el| {
        if matches_query(el, bindings, query) {
            matches.push(StructuralMatch {
                span: el.span,
                tag: bindings.classify_lenient(&el.name).as_str().to_string(),
            });
        }
    });
    for_each_declaration(doc, |declaration| {
        if matches_declaration_query(declaration, bindings, query) {
            matches.push(StructuralMatch {
                span: declaration.span,
                tag: bindings
                    .classify_attribute_lenient(&declaration.name)
                    .as_str()
                    .to_string(),
            });
        }
    });
    matches.sort_by_key(|m| m.span.start);
    matches
}

fn tuple_contains_str(tuple: &Tuple, sub: &str) -> bool {
    tuple.entries().any(|e| {
        e.name().is_some_and(|name| name.contains(sub)) || value_contains_str(e.value(), sub)
    })
}

#[deny(clippy::wildcard_enum_match_arm)]
fn value_contains_str(v: &Value, sub: &str) -> bool {
    match v {
        Value::Scalar(s) => match s {
            Scalar::Text(s) | Scalar::Number(s) => s.contains(sub),
            Scalar::None | Scalar::Auto | Scalar::Bool(_) | Scalar::Int(_) | Scalar::Float(_) => {
                false
            }
        },
        Value::Tuple(tuple) => tuple_contains_str(tuple, sub),
        Value::Call(name, args) => name.contains(sub) || tuple_contains_str(args, sub),
        Value::Interp(it) => it.expr.to_string().contains(sub),
        Value::TypeSeat(seat) => seat.expr.to_string().contains(sub),
        Value::Element(_) | Value::Hole(_) | Value::Blocks(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast_parser::parse_file;

    /// What a document read with no vault has in scope: the vocabularies tomet carries.
    fn carried(doc: &File) -> Bindings {
        tomet_semantics::LoadedVocabularies::default().bindings_for(doc)
    }

    #[test]
    fn matches_query_by_tag() {
        let doc = parse_file("::meta(author: \"Alice\")\n#[ Title ]\n").unwrap();
        let query = StructuralQuery {
            tag: Some("meta".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &carried(&doc), &query), 1);
        let matches = find_structural_matches(&doc, &carried(&doc), &query);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tag, "meta");
    }

    #[test]
    fn matches_query_by_key() {
        let doc = parse_file("::meta(author: \"Alice\", draft: true)\n").unwrap();
        let q_author = StructuralQuery {
            key: Some("author".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &carried(&doc), &q_author), 1);

        let q_missing = StructuralQuery {
            key: Some("nonexistent".to_string()),
            ..Default::default()
        };
        assert_eq!(
            count_structural_matches(&doc, &carried(&doc), &q_missing),
            0
        );
    }

    #[test]
    fn matches_query_by_value_contains() {
        let doc = parse_file("::meta(author: \"Alice Wonderland\")\n").unwrap();
        let q_val = StructuralQuery {
            value_contains: Some("Wonder".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &carried(&doc), &q_val), 1);

        let q_nomatch = StructuralQuery {
            value_contains: Some("Bob".to_string()),
            ..Default::default()
        };
        assert_eq!(
            count_structural_matches(&doc, &carried(&doc), &q_nomatch),
            0
        );
    }

    #[test]
    fn matches_query_by_value_contains_in_call_and_interp() {
        let doc =
            parse_file("::meta(rule: allow:(\"special-item\"), expr: $(vault.name))\n").unwrap();
        let q_call = StructuralQuery {
            value_contains: Some("special".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &carried(&doc), &q_call), 1);

        let q_interp = StructuralQuery {
            value_contains: Some("vault".to_string()),
            ..Default::default()
        };
        assert_eq!(count_structural_matches(&doc, &carried(&doc), &q_interp), 1);
    }

    /// The names `doc` has in scope in a vault holding a `node-graph` vocabulary.
    fn with_node_graph(doc: &File) -> Bindings {
        let mut loaded = tomet_semantics::LoadedVocabularies::default();
        loaded.add("node-graph", &parse_file(NODE_GRAPH).unwrap());
        assert!(loaded.errors.is_empty(), "{:?}", loaded.errors);
        loaded.bindings_for(doc)
    }

    const NODE_GRAPH: &str = "::kind(vocabulary)\n::vocabulary(node-graph, version: \"1.0.0\")\n::element(\n  node: ()\n)\n::attribute(\n  layout: ()\n)\n";

    /// A name written under a `::use` alias is matched, and reported, in its vocabulary's own
    /// namespace, exactly as one written out, or bare under the document's kind, is.
    #[test]
    fn an_aliased_name_matches_in_its_real_namespace() {
        let query = |tag: &str| StructuralQuery {
            tag: Some(tag.to_string()),
            ..Default::default()
        };
        for src in [
            "::use(node-graph, as: ng)\n::ng.layout(tree)\n\n@ng.node(a)\n",
            "::use(node-graph)\n::node-graph.layout(tree)\n\n@node-graph.node(a)\n",
            // bare under the document's own kind: resolves only with the vocabulary in hand
            "::kind(node-graph)\n::layout(tree)\n\n@node(a)\n",
        ] {
            let doc = parse_file(src).unwrap();
            let bindings = with_node_graph(&doc);
            let found = find_structural_matches(&doc, &bindings, &query("node-graph.node"));
            assert_eq!(found.len(), 1, "{src}");
            assert_eq!(found[0].tag, "node-graph.node");
            let found = find_structural_matches(&doc, &bindings, &query("node-graph.layout"));
            assert_eq!(found.len(), 1, "{src}");
            assert_eq!(
                count_structural_matches(&doc, &bindings, &query("ng.node")),
                0
            );
        }
    }
}
