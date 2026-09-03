//! Directive promotion and Value DSL normalization transformations.

use tomet_ast::{Block, Document, Element, ElementValue, Sigil, Value};
use tomet_semantics::{ElementKind, classify_lenient};
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
                // A `+++` fence body is opaque text, so there is no key to
                // remove from it. Read it through its declared `format:`
                // and rewrite it as a native group first -- otherwise this
                // silently finds nothing whenever the source element was
                // written as a fence, and depends on the value-DSL
                // normalization happening to have run first.
                materialize_raw_body(el);
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
            let version_idx = doc.find_element_block_index(|el| el.sigil.is_bare_named("version"));
            match version_idx {
                Some(v_idx) => v_idx + 1,
                None => 0,
            }
        }
    };

    let mut new_directive = element_new(Sigil::named(target_directive_name));
    new_directive.args = Some(prop_val);

    doc.insert_block(insert_pos, Block::Element(new_directive));
    true
}

/// Rewrites an element's `+++` fence body as a native value group, when
/// its declared `format:` says how to read it. A no-op otherwise.
fn materialize_raw_body(el: &mut Element) {
    if !matches!(el.value, Some(ElementValue::Raw(_))) {
        return;
    }
    if let Some(data) = tomet_semantics::embedded::element_data(el) {
        el.value = Some(ElementValue::from_map(data));
    }
}

/// Normalizes `#meta` elements in `doc` from `format:yaml` (or another
/// embedded format) to the native Value DSL.
///
/// Reads the body through `embedded::element_data` *before* stripping the
/// `format` argument, and rewrites it as a native group -- otherwise the
/// body stays an opaque `+++` fence with nothing left to say how to read
/// it, and every later pass sees an element with no data.
pub fn normalize_meta_to_value_dsl(doc: &mut Document) -> bool {
    let mut changed = false;
    for_each_element_mut(doc, |el| {
        let kind = classify_lenient(el);
        if kind == ElementKind::Meta {
            if matches!(el.value, Some(ElementValue::Raw(_))) {
                materialize_raw_body(el);
                changed = !matches!(el.value, Some(ElementValue::Raw(_))) || changed;
            }
            if let Some(Value::Map(entries)) = &mut el.args {
                if let Some(pos) = entries.iter().position(|(k, _)| k == "format") {
                    entries.remove(pos);
                    changed = true;
                }
                if entries.is_empty() {
                    el.args = None;
                    changed = true;
                }
            } else if matches!(&el.args, Some(Value::String(fmt)) if fmt == "yaml" || fmt == "json" || fmt == "toml")
            {
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
        |el| classify_lenient(el) == ElementKind::Meta,
        "type",
        "kind",
        None,
    )
}
