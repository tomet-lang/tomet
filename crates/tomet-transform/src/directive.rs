//! Directive promotion and Value DSL normalization transformations.

use tomet_ast::{Declaration, File, Value};
use tomet_semantics::DeclKind;
use tomet_semantics::{ElementKind, classify_declaration_lenient};
use tomet_tree::{DataExt, declaration_new, for_each_declaration_mut};

/// Promotes a property `prop_key` from a declaration matching `source_filter` into a new top-level
/// declaration `::target_directive_name(val)`, inserting it at `target_index`
/// (or automatically after any `::version` directive, or at position 0).
///
/// If the property is absent, empty, or null, it is simply removed without creating a directive.
/// Returns `true` if a directive was created and inserted.
pub fn promote_prop_to_directive<F>(
    doc: &mut File,
    mut source_filter: F,
    prop_key: &str,
    target_directive_name: &str,
    target_index: Option<usize>,
) -> bool
where
    F: FnMut(&Declaration) -> bool,
{
    let mut extracted_val = None;
    for_each_declaration_mut(doc, |declaration| {
        if extracted_val.is_some() || !source_filter(declaration) {
            return;
        }
        extracted_val = declaration.data.remove_prop(prop_key);
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
        Some(idx) => idx.min(doc.header.len()),
        None => {
            let version_idx = doc
                .header
                .iter()
                .position(|d| d.name.is_bare_named("version"));
            match version_idx {
                Some(v_idx) => v_idx + 1,
                None => 0,
            }
        }
    };

    let mut new_directive = declaration_new(target_directive_name);
    new_directive.data.args = Some(prop_val);

    doc.header.insert(insert_pos, new_directive);
    true
}

/// Promotes `::meta`'s `id:` field to a top-level `::id(...)` element, after the `::version` and
/// `::kind` that lead the document.
pub fn promote_meta_id_to_id(doc: &mut File) -> bool {
    let after_preamble = ["kind", "version"]
        .iter()
        .filter_map(|name| doc.header.iter().position(|d| d.name.is_bare_named(name)))
        .max()
        .map_or(0, |index| index + 1);
    promote_prop_to_directive(
        doc,
        |d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta),
        "id",
        "id",
        Some(after_preamble),
    )
}

/// Promotes `::meta`'s `type:` field to a top-level `::kind(...)` element.
pub fn promote_meta_type_to_kind(doc: &mut File) -> bool {
    promote_prop_to_directive(
        doc,
        |d| classify_declaration_lenient(d) == ElementKind::Declaration(DeclKind::Meta),
        "type",
        "kind",
        None,
    )
}
