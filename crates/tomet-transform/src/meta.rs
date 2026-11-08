//! Metadata element manipulation in documents.

use tomet_ast::{File, Value};
use tomet_semantics::{classify_declaration_lenient, classify_std_lenient};
use tomet_tree::{DataExt, declaration_new, for_each_declaration_mut, for_each_element_mut};

/// Updates or inserts a key-value pair in a target directive/metadata block (e.g. `::meta`).
/// If the target block is not found, prepends a new directive block at position 0.
pub fn set_meta_in_doc(doc: &mut File, target_element: &str, key: &str, new_val: &str) -> bool {
    let mut found = false;

    for_each_element_mut(doc, |el| {
        let kind = classify_std_lenient(el.as_view());
        if kind.as_str() == target_element {
            found = true;
            el.data.set_prop(key, Value::String(new_val.to_string()));
        }
    });
    for_each_declaration_mut(doc, |declaration| {
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
        declaration.data.args = Some(Value::Map(vec![(
            key.to_string(),
            Value::String(new_val.to_string()),
        )]));
        doc.header.insert(0, declaration);
    }

    true
}
