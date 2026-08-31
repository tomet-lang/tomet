//! Directive promotion and Value DSL normalization transformations.

use tomet_ast::{Block, Document, Element, Sigil, Value};
use tomet_semantics::{classify, ElementKind};
use tomet_tree::{DocumentExt, ElementExt, element_new, for_each_element_mut};

/// Promotes a property `prop_key` from an element matching `source_filter` into a new top-level
/// directive `@target_directive_name(val)` (or `@target_directive_name{...}`), inserting it at `target_index`
/// (or automatically after any `@version` directive, or at position 0).
///
/// If the property is absent, empty, or null, it is simply removed without creating a directive.
/// Returns `true` if a directive was created and inserted.
pub fn promote_prop_to_directive<F>(
    doc: &mut Document,
    mut source_filter: F,
    prop_key: &str,
    target_directive_name: &str,
    target_index: Option<usize>,
) -> bool
where
    F: FnMut(&Element) -> bool,
{
    let mut extracted_val = None;
    for block in &mut doc.blocks {
        if let Block::Element(el) = block {
            if source_filter(el) {
                extracted_val = el.remove_prop(prop_key);
                break;
            }
        }
    }

    let Some(prop_val) = extracted_val else {
        return false;
    };

    let is_empty = match &prop_val {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Seq(items) => items.is_empty(),
        Value::Map(entries) => entries.is_empty(),
        _ => false,
    };

    if is_empty {
        return false;
    }

    let insert_pos = match target_index {
        Some(idx) => idx,
        None => {
            let version_idx = doc.find_element_block_index(|el| {
                matches!(&el.sigil, Sigil::At(Some(name)) if name == "version")
            });
            match version_idx {
                Some(v_idx) => v_idx + 1,
                None => 0,
            }
        }
    };

    let mut new_directive = element_new(Sigil::At(Some(target_directive_name.to_string())));
    new_directive.args = Some(prop_val);

    doc.insert_block(insert_pos, Block::Element(new_directive));
    true
}

/// Normalizes `@meta` elements in `doc` from `format:yaml` (or other embedded format) to native Value DSL.
/// Strips the `format` argument from `@meta(...)` and ensures clean map representation.
pub fn normalize_meta_to_value_dsl(doc: &mut Document) -> bool {
    let mut changed = false;
    for_each_element_mut(doc, |el| {
        let kind = classify(el);
        if kind == ElementKind::Meta {
            if let Some(Value::Map(entries)) = &mut el.args {
                if let Some(pos) = entries.iter().position(|(k, _)| k == "format") {
                    entries.remove(pos);
                    changed = true;
                }
                if entries.is_empty() {
                    el.args = None;
                    changed = true;
                }
            } else if matches!(&el.args, Some(Value::String(fmt)) if fmt == "yaml" || fmt == "json" || fmt == "toml") {
                el.args = None;
                changed = true;
            }
        }
    });
    changed
}

/// Promotes `@meta`'s `type:` field to a top-level `@kind(...)` element.
pub fn promote_meta_type_to_kind(doc: &mut Document) -> bool {
    promote_prop_to_directive(
        doc,
        |el| classify(el) == ElementKind::Meta,
        "type",
        "kind",
        None,
    )
}
