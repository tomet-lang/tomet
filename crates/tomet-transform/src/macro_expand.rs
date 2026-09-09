//! Forward macro expansion and dollar expression evaluation.

use tomet_ast::{Document, ElementValue, Value};
use tomet_semantics::DocumentConfig;
use tomet_tree::for_each_element_mut;

/// Expands `$macro(...)` or `${...}` expressions within element arguments (e.g. `@link($macro.https(...))`)
/// using the provided [`DocumentConfig`].
pub fn expand_document_macros(doc: &mut Document, config: &DocumentConfig) {
    let doc_clone = doc.clone();
    for_each_element_mut(doc, |el| {
        if let Some(args) = &mut el.args {
            expand_value_macros(&doc_clone, args, config);
        }
    });
}

fn expand_value_macros(doc: &Document, value: &mut Value, config: &DocumentConfig) {
    match value {
        Value::String(s) => {
            if let Some(expanded) = try_eval_dollar_expr(doc, s, config) {
                *value = expanded;
            }
        }
        Value::Seq(items) => {
            for item in items {
                expand_value_macros(doc, item, config);
            }
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                expand_value_macros(doc, v, config);
            }
        }
        _ => {}
    }
}

fn try_eval_dollar_expr(doc: &Document, s: &str, config: &DocumentConfig) -> Option<Value> {
    let trimmed = s.trim();
    if !trimmed.starts_with('$') {
        return None;
    }
    let el = tomet_parser::parse_dollar_element_str(trimmed).ok()?;
    if let Some(ElementValue::Interp(expr)) = el.value {
        tomet_compute::evaluate_with_config(doc, &expr, config).ok()
    } else {
        None
    }
}
