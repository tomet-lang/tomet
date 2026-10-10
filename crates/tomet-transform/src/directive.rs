//! Directive promotion and Value DSL normalization transformations.

use tomet_ast::{Block, Document, Element, Sigil, Value};
use tomet_semantics::{ElementKind, classify_std_lenient};
use tomet_tree::{DocumentExt, ElementExt, element_new, for_each_top_level_element_mut};

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
    // Top-level per `for_each_top_level_element_mut`, not a raw
    // `doc.blocks` walk: the source element (`@meta`, typically) may have
    // joined an adjacent paragraph (`docs/spec/syntax.tmt`'s
    // `##[ 区切り ]`) and come back as `Inline::Element` there instead of
    // its own `Block::Element` -- still top-level, just a different
    // tree shape.
    let mut extracted_val = None;
    for_each_top_level_element_mut(doc, |el| {
        if extracted_val.is_some() || !source_filter(el) {
            return;
        }
        extracted_val = el.remove_prop(prop_key);
    });

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

/// Promotes `@meta`'s `type:` field to a top-level `@kind(...)` element.
pub fn promote_meta_type_to_kind(doc: &mut Document) -> bool {
    promote_prop_to_directive(
        doc,
        |el| classify_std_lenient(el) == ElementKind::Meta,
        "type",
        "kind",
        None,
    )
}
