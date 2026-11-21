//! Metadata element manipulation in documents.

use tomet_ast::{File, Tuple, Value};
use tomet_scalar::Scalar;
use tomet_semantics::Bindings;
use tomet_tree::{DataExt, declaration_new, for_each_declaration_mut, for_each_element_mut};

/// Updates or inserts a key-value pair in a target directive/metadata block (e.g. `::meta`).
/// If the target block is not found, prepends a new directive block at position 0.
///
/// `target_element` is matched against what each name resolves to in `bindings` (the names `doc`
/// has in scope): a vocabulary's attribute is named in its own namespace, whatever alias the
/// document writes it under.
pub fn set_meta_in_doc(
    doc: &mut File,
    bindings: &Bindings,
    target_element: &str,
    key: &str,
    new_val: &str,
) -> bool {
    let mut found = false;

    for_each_element_mut(doc, |el| {
        let kind = bindings.classify_lenient(&el.name);
        if kind.as_str() == target_element {
            found = true;
            el.data
                .set_prop(key, Value::Scalar(Scalar::Text(new_val.to_string())));
        }
    });
    for_each_declaration_mut(doc, |declaration| {
        let kind = bindings.classify_attribute_lenient(&declaration.name);
        if kind.as_str() == target_element {
            found = true;
            declaration
                .data
                .set_prop(key, Value::Scalar(Scalar::Text(new_val.to_string())));
        }
    });

    if !found {
        // A directive inserted at the top of the document is a declaration,
        // which stands alone on its line.
        let mut declaration = declaration_new(target_element);
        declaration.data.args = Some(Tuple::from_named([(
            key.to_string(),
            Value::Scalar(Scalar::Text(new_val.to_string())),
        )]));
        doc.header.insert(0, declaration);
    }

    true
}
