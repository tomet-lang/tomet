//! ID generation and rendering helpers.

use tomet_ast::{Block, Document, ElementValue, Entry, Id, Sigil, Value};
use tomet_config::PrinterConfig;
use tomet_field_utils::{generate_id_for_field, is_valid_id_format};
use tomet_style::quote_scalar_string;
use tomet_tree::element_new;

/// Ensures that a document has an `id` field in its `@meta` block if configured to do so.
pub fn ensure_document_id_with_config(doc: &mut Document, config: &PrinterConfig) {
    let Some(id_cfg) = config.meta.fields.get("id") else {
        return;
    };
    if id_cfg.field_type.is_none() {
        return;
    }

    let force = id_cfg.force.unwrap_or(true);
    let overwrite = id_cfg.overwrite.unwrap_or(false);

    let mut meta_found = false;
    for block in &mut doc.blocks {
        if let Block::Element(el) = block
            && el.sigil.is_bare_named("meta")
        {
            meta_found = true;
            if let Some(ElementValue::Group(entries)) = &mut el.value {
                let existing_idx =
                    entries
                        .iter()
                        .enumerate()
                        .find_map(|(idx, entry)| match entry {
                            Entry::Pair(k, v) if k == "id" => Some((idx, v.clone())),
                            _ => None,
                        });

                if let Some((idx, val)) = existing_idx {
                    if overwrite {
                        let existing_str = match &val {
                            Value::String(s) => s.as_str(),
                            _ => "",
                        };
                        if !is_valid_id_format(existing_str, id_cfg) {
                            let new_id = generate_id_for_field(id_cfg);
                            entries[idx] = Entry::Pair("id".to_string(), Value::String(new_id));
                        }
                    }
                } else if force || overwrite {
                    let new_id = generate_id_for_field(id_cfg);
                    entries.insert(0, Entry::Pair("id".to_string(), Value::String(new_id)));
                }
            }
            break;
        }
    }

    if !meta_found && (force || overwrite) {
        let new_id = generate_id_for_field(id_cfg);
        let mut meta_el = element_new(Sigil::named("meta"));
        meta_el.value = Some(ElementValue::Group(vec![Entry::Pair(
            "id".to_string(),
            Value::String(new_id),
        )]));
        doc.blocks.insert(0, Block::Element(meta_el));
    }
}

/// Prints `#(foobar)` if `id` is `Some`, else nothing. Shared by every
/// construct that carries an id (elements, sections, list items,
/// connects) -- always printed right after the construct's own
/// `(args)[content]{value}` and before any connect, matching where the
/// parser reads it.
pub(crate) fn render_id(id: Option<&Id>) -> String {
    match id {
        Some(id) => format!("#({})", quote_scalar_string(&id.0)),
        None => String::new(),
    }
}
