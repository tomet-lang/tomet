//! Footnote collection, sequential numbering, and cross-reference resolution.

use crate::kind::{ElementKind, classify_std_lenient};
use crate::positional::normalized_element_args;
use std::collections::HashMap;
use tomet_ast::{Document, Element, Placement, Span};
use tomet_tree::{ValueExt, for_each_element};

/// An individual footnote entry resolved within a document.
#[derive(Debug, Clone)]
pub struct FootnoteItem {
    /// 1-based sequential number (1, 2, 3...)
    pub index: usize,
    /// Target ID if named (e.g. "note1")
    pub id: Option<String>,
    /// The footnote definition element, if found.
    pub definition: Option<Element>,
    /// Backlink anchor IDs for occurrences in prose (e.g. ["fnref-1-1", "fnref-1-2"])
    pub backlinks: Vec<String>,
}

/// Key derived from a Span's start and end byte offsets for HashMap lookups.
fn span_key(span: &Span) -> (usize, usize) {
    (span.start, span.end)
}

/// Registry storing all resolved footnotes and element back-references for a document.
#[derive(Debug, Default)]
pub struct FootnoteRegistry {
    pub items: Vec<FootnoteItem>,
    /// Map from element span offset range to (footnote_index, backlink_anchor_id)
    span_to_ref: HashMap<(usize, usize), (usize, String)>,
}

/// Counts one more occurrence of footnote `id` in prose, numbering it if it is new.
///
/// The definition is the element with content that carries the id; the first
/// pass found them all, so a reference may come before its definition.
fn note_reference(
    registry: &mut FootnoteRegistry,
    id_to_index: &mut HashMap<String, usize>,
    defs: &HashMap<String, Element>,
    id: String,
    span: &Span,
) {
    let index = if let Some(&idx) = id_to_index.get(&id) {
        idx
    } else {
        let idx = registry.items.len() + 1;
        let definition = defs.get(&id).cloned();
        id_to_index.insert(id.clone(), idx);
        registry.items.push(FootnoteItem {
            index: idx,
            id: Some(id),
            definition,
            backlinks: Vec::new(),
        });
        idx
    };
    let item = &mut registry.items[index - 1];
    let ref_num = item.backlinks.len() + 1;
    let backlink_id = format!("fnref-{}-{}", index, ref_num);
    item.backlinks.push(backlink_id.clone());
    registry
        .span_to_ref
        .insert(span_key(span), (index, backlink_id));
}

impl FootnoteRegistry {
    /// Scans `doc` in document order and resolves all footnotes and references.
    pub fn from_document(doc: &Document) -> Self {
        // Pass 1: Collect definitions, `@ftn(id)[text]`. One without content,
        // `@ftn(id)`, is a reference to one and defines nothing.
        let mut defs: HashMap<String, Element> = HashMap::new();
        for_each_element(doc, |el| {
            if classify_std_lenient(el) == ElementKind::Footnote
                && el.content.is_some()
                && let Some(id) = get_element_id(el)
            {
                defs.insert(id, el.clone());
            }
        });

        // Pass 2: In-order traversal of occurrences in prose
        let mut registry = Self::default();
        let mut id_to_index: HashMap<String, usize> = HashMap::new();

        for_each_element(doc, |el| {
            let kind = classify_std_lenient(el);
            match kind {
                ElementKind::Footnote => {
                    // Block-placed footnote with ID and content is a separated
                    // definition block, not an in-text reference occurrence.
                    let is_block_def = el.placement == Placement::Block
                        && el.content.is_some()
                        && get_element_id(el).is_some();
                    if is_block_def {
                        return;
                    }

                    let id = get_element_id(el);
                    if let Some(id_str) = id {
                        if el.content.is_none() {
                            // `@ftn(id)`: a reference to the definition with that id
                            note_reference(
                                &mut registry,
                                &mut id_to_index,
                                &defs,
                                id_str,
                                &el.span,
                            );
                            return;
                        }
                        let index = if let Some(&idx) = id_to_index.get(&id_str) {
                            idx
                        } else {
                            let idx = registry.items.len() + 1;
                            id_to_index.insert(id_str.clone(), idx);
                            registry.items.push(FootnoteItem {
                                index: idx,
                                id: Some(id_str.clone()),
                                definition: Some(el.clone()),
                                backlinks: Vec::new(),
                            });
                            idx
                        };
                        let item = &mut registry.items[index - 1];
                        let ref_num = item.backlinks.len() + 1;
                        let backlink_id = format!("fnref-{}-{}", index, ref_num);
                        item.backlinks.push(backlink_id.clone());
                        registry
                            .span_to_ref
                            .insert(span_key(&el.span), (index, backlink_id));
                    } else {
                        // Unnamed inline footnote: always a fresh sequential number
                        let index = registry.items.len() + 1;
                        let backlink_id = format!("fnref-{}-1", index);
                        registry.items.push(FootnoteItem {
                            index,
                            id: None,
                            definition: Some(el.clone()),
                            backlinks: vec![backlink_id.clone()],
                        });
                        registry
                            .span_to_ref
                            .insert(span_key(&el.span), (index, backlink_id));
                    }
                }
                _ => {}
            }
        });

        registry
    }

    /// Look up reference info (index, backlink_id) by element span.
    pub fn get_ref(&self, span: &Span) -> Option<(usize, &str)> {
        self.span_to_ref
            .get(&span_key(span))
            .map(|(idx, backlink)| (*idx, backlink.as_str()))
    }
}

fn get_element_id(el: &Element) -> Option<String> {
    let args = normalized_element_args(el)?;
    if let Some(id) = args.get("id") {
        return value_to_id(id);
    }
    value_to_id(&args)
}

fn value_to_id(val: &tomet_ast::Value) -> Option<String> {
    match val {
        tomet_ast::Value::String(s) => Some(s.clone()),
        tomet_ast::Value::Int(i) => Some(i.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_parser::parse_document;

    #[test]
    fn test_inline_footnote_numbering() {
        let src = "First@ftn[one] and second@ftn[two].\n";
        let doc = parse_document(src).unwrap();
        let registry = FootnoteRegistry::from_document(&doc);

        assert_eq!(registry.items.len(), 2);
        assert_eq!(registry.items[0].index, 1);
        assert_eq!(registry.items[0].id, None);
        assert_eq!(registry.items[1].index, 2);
        assert_eq!(registry.items[1].id, None);
    }

    #[test]
    fn test_numeric_footnote_id_resolution() {
        let src = "Item @ftn(1) and @ftn(2).\n\n@ftn(1)[Note 1]\n@ftn(2)[Note 2]\n";
        let doc = parse_document(src).unwrap();
        let registry = FootnoteRegistry::from_document(&doc);

        assert_eq!(registry.items.len(), 2);
        assert_eq!(registry.items[0].index, 1);
        assert_eq!(registry.items[0].id.as_deref(), Some("1"));
        assert!(registry.items[0].definition.is_some());

        assert_eq!(registry.items[1].index, 2);
        assert_eq!(registry.items[1].id.as_deref(), Some("2"));
        assert!(registry.items[1].definition.is_some());
    }

    /// The definition and the reference are the same
    /// element, told apart by whether it has content.
    #[test]
    fn ftn_defines_with_content_and_refers_without() {
        let src = "See @ftn(intro) and @ftn(intro) and @ftn(other).\n\n@ftn(intro)[Introduction text]\n\n@ftn(other)| Other text\n";
        let doc = parse_document(src).unwrap();
        let registry = FootnoteRegistry::from_document(&doc);

        assert_eq!(registry.items.len(), 2);
        assert_eq!(registry.items[0].index, 1);
        assert_eq!(registry.items[0].id.as_deref(), Some("intro"));
        assert_eq!(registry.items[0].backlinks.len(), 2);
        let def = registry.items[0].definition.as_ref().expect("a definition");
        assert!(
            def.content.is_some(),
            "the definition is the one with content"
        );

        assert_eq!(registry.items[1].index, 2);
        assert_eq!(registry.items[1].id.as_deref(), Some("other"));
        assert_eq!(registry.items[1].backlinks.len(), 1);
        assert!(registry.items[1].definition.is_some());
    }

    #[test]
    fn a_reference_before_its_definition_and_a_missing_one_are_both_fine() {
        let src = "A@ftn(later) and B@ftn(nowhere).\n\n@ftn(later)[Defined below]\n";
        let registry = FootnoteRegistry::from_document(&parse_document(src).unwrap());
        assert_eq!(registry.items.len(), 2);
        assert!(registry.items[0].definition.is_some());
        assert!(registry.items[1].definition.is_none());
    }

    #[test]
    fn ftn_inline_with_content_is_an_unnamed_footnote() {
        let src = "First@ftn[one] and second@ftn[two].\n";
        let registry = FootnoteRegistry::from_document(&parse_document(src).unwrap());
        assert_eq!(registry.items.len(), 2);
        assert_eq!(registry.items[0].id, None);
    }
}
