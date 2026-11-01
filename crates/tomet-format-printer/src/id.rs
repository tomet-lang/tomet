//! ID generation and rendering helpers.

use tomet_ast::{Declaration, File, Id, Value};
use tomet_config::PrinterConfig;
use tomet_field_utils::{generate_id_for_field, is_valid_id_format};
use tomet_style::quote_scalar_string;
use tomet_tree::{ValueExt, declaration_new};

/// Ensures that a document has an `::id(...)` declaration if configured to do so.
///
/// The generation is configured by the vault's `id` meta field (`type`, `force`, `overwrite`, ...).
/// A new declaration goes after the leading `::version` / `::kind`, so the preamble reads
/// `::kind`, `::id`, `::meta`.
pub fn ensure_document_id_with_config(doc: &mut File, config: &PrinterConfig) {
    let Some(id_cfg) = config.meta.fields.get("id") else {
        return;
    };
    if id_cfg.field_type.is_none() {
        return;
    }

    let force = id_cfg.force.unwrap_or(true);
    let overwrite = id_cfg.overwrite.unwrap_or(false);

    let mut found = false;
    for declaration in &mut doc.header {
        if found || !declaration.name.is_bare_named("id") {
            continue;
        }
        found = true;
        if overwrite && !declared_id(declaration).is_some_and(|id| is_valid_id_format(id, id_cfg)) {
            declaration.data.args = Some(Value::String(generate_id_for_field(id_cfg)));
        }
    }

    if !found && (force || overwrite) {
        let at = ["kind", "version"]
            .iter()
            .filter_map(|name| doc.header.iter().position(|d| d.name.is_bare_named(name)))
            .max()
            .map_or(0, |index| index + 1);
        let mut declaration = declaration_new("id");
        declaration.data.args = Some(Value::String(generate_id_for_field(id_cfg)));
        doc.header.insert(at, declaration);
    }
}

/// The identity an `::id(...)` declaration holds: `::id(x)` or `::id(id: x)`.
fn declared_id(declaration: &Declaration) -> Option<&str> {
    let args = declaration.data.args.as_ref()?;
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
