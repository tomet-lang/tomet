//! Forward macro expansion and dollar expression evaluation.

use tomet_ast::{Document, ElementValue, Sigil, Value};
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
        // `$macro.https(...)` in a value position (`@link($macro.https(...))`)
        // now parses directly to this, rather than to a `Value::String`
        // `try_eval_dollar_expr` has to re-parse -- evaluate it in place
        // and splice in the result, same as that string path does.
        Value::Element(el) if el.sigil == Sigil::Dollar => {
            if let Some(ElementValue::Interp(expr)) = &el.value {
                if let Ok(evaluated) = tomet_compute::evaluate_with_config(doc, expr, config) {
                    *value = evaluated;
                }
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
