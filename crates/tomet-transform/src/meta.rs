//! Metadata element manipulation in documents.

use tomet_ast::{Block, Document, Group, Value};
use tomet_semantics::{classify_declaration_lenient, classify_std_lenient};
use tomet_tree::{
    DataExt, declaration_new, for_each_element_mut, for_each_top_level_declaration_mut,
};

/// Updates or inserts a key-value pair in a target directive/metadata block (e.g. `::meta`).
/// If the target block is not found, prepends a new directive block at position 0.
pub fn set_meta_in_doc(doc: &mut Document, target_element: &str, key: &str, new_val: &str) -> bool {
    let mut found = false;

    for_each_element_mut(doc, |el| {
        let kind = classify_std_lenient(el);
        if kind.as_str() == target_element {
            found = true;
            el.data.set_prop(key, Value::String(new_val.to_string()));
        }
    });
    for_each_top_level_declaration_mut(doc, |declaration| {
        let kind = classify_declaration_lenient(declaration);
        if kind.as_str() == target_element {
            found = true;
            declaration
                .data
                .set_prop(key, Value::String(new_val.to_string()));
        }
    });

    if !found {
        // A directive inserted at the top of the document is a declaration,
        // which stands alone on its line.
        let mut declaration = declaration_new(target_element);
        declaration.data.value = Some(Group::from_map(Value::Map(vec![(
            key.to_string(),
            Value::String(new_val.to_string()),
        )])));
        doc.blocks.insert(0, Block::Declaration(declaration));
    }

    true
}
