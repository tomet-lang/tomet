//! Metadata element manipulation in documents.

use tomet_ast::{Block, Document, Element, ElementValue, Sigil, Value};
use tomet_semantics::classify_lenient;
use tomet_tree::{ElementExt, for_each_element_mut};

/// Updates or inserts a key-value pair in a target directive/metadata block (e.g. `@meta`).
/// If the target block is not found, prepends a new directive block at position 0.
pub fn set_meta_in_doc(doc: &mut Document, target_element: &str, key: &str, new_val: &str) -> bool {
    let mut found = false;

    for_each_element_mut(doc, |el| {
        let kind = classify_lenient(el);
        if kind.as_str() == target_element {
            found = true;
            el.set_prop(key, Value::String(new_val.to_string()));
        }
    });

    if !found {
        let new_el = Element {
            sigil: Sigil::block(target_element),
            args: None,
            content: None,
            children: None,
            value: Some(ElementValue::from_map(Value::Map(vec![(
                key.to_string(),
                Value::String(new_val.to_string()),
            )]))),
            span: Default::default(),
        };
        doc.blocks.insert(0, Block::Element(new_el));
    }

    true
}
