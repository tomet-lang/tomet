//! ID generation and rendering helpers.

use tomet_ast::{Block, Document, Element, Id, Sigil, Value};
use tomet_config::PrinterConfig;
use tomet_field_utils::{generate_id_for_field, is_valid_id_format};
use tomet_style::quote_scalar_string;
use tomet_tree::{DocumentExt, ValueExt, element_new, for_each_top_level_element_mut};

/// Ensures that a document has an `::id(...)` declaration if configured to do so.
///
/// The generation is configured by the vault's `id` meta field (`type`, `force`, `overwrite`, ...).
/// A new declaration goes after the leading `::version` / `::kind`, so the preamble reads
/// `::kind`, `::id`, `::meta`.
pub fn ensure_document_id_with_config(doc: &mut Document, config: &PrinterConfig) {
    let Some(id_cfg) = config.meta.fields.get("id") else {
        return;
    };
    if id_cfg.field_type.is_none() {
        return;
    }

    let force = id_cfg.force.unwrap_or(true);
    let overwrite = id_cfg.overwrite.unwrap_or(false);

    // Top-level, not a walk of `doc.blocks`: `::kind` and `::id` on adjacent lines join into one
    // paragraph, and an `::id` there is still the document's.
    let mut found = false;
    for_each_top_level_element_mut(doc, |el| {
        if found || !el.sigil.is_bare_named("id") {
            return;
        }
        found = true;
        if overwrite && !declared_id(el).is_some_and(|id| is_valid_id_format(id, id_cfg)) {
            el.args = Some(Value::String(generate_id_for_field(id_cfg)));
        }
    });

    if !found && (force || overwrite) {
        let at = ["kind", "version"]
            .iter()
            .filter_map(|name| doc.find_element_block_index(|el| el.sigil.is_bare_named(name)))
            .max()
            .map_or(0, |index| index + 1);
        let mut el = element_new(Sigil::document("id"));
        el.args = Some(Value::String(generate_id_for_field(id_cfg)));
        doc.insert_block(at, Block::Element(el));
    }
}

/// The identity an `::id(...)` element holds: `::id(x)` or `::id(id: x)`.
fn declared_id(el: &Element) -> Option<&str> {
    let args = el.args.as_ref()?;
    args.as_str()
        .or_else(|| args.get("id").and_then(|id| id.as_str()))
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
