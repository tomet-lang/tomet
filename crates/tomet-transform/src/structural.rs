//! Structural AST query matching and in-place transformations.

use tomet_ast::{Document, Element, ElementValue, Sigil, Value};
use tomet_semantics::classify_lenient;
use tomet_tree::{ElementExt, for_each_element, for_each_element_mut};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructuralQuery {
    pub tag: Option<String>,
    pub key: Option<String>,
    pub value_contains: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralAction {
    RenameTag { from: String, to: String },
    RenameKey { old_key: String, new_key: String },
    ReplaceValue { key: String, new_value: String },
}

/// Checks if an element matches the given query.
pub fn matches_query(el: &Element, query: &StructuralQuery) -> bool {
    let kind = classify_lenient(el);
    if let Some(target_tag) = &query.tag {
        if !target_tag.is_empty() && !kind.as_str().eq_ignore_ascii_case(target_tag) {
            return false;
        }
    }

    if let Some(target_key) = &query.key {
        if !target_key.is_empty() && !el.has_prop_key(target_key) {
            return false;
        }
    }

    if let Some(sub) = &query.value_contains {
        if !sub.is_empty() {
            let in_args = el
                .args
                .as_ref()
                .map_or(false, |v| value_contains_str(v, sub));
            let in_val = el.value.as_ref().map_or(false, |v| match v {
                ElementValue::Group(_) => v
                    .as_data()
                    .is_some_and(|data| value_contains_str(&data, sub)),
                ElementValue::Raw(body) => body.contains(sub),
                ElementValue::Interp(_) => false,
            });
            if !in_args && !in_val {
                return false;
            }
        }
    }

    true
}

/// Counts matching elements in `doc` for the given query.
pub fn count_structural_matches(doc: &Document, query: &StructuralQuery) -> usize {
    let mut count = 0;
    for_each_element(doc, |el| {
        if matches_query(el, query) {
            count += 1;
        }
    });
    count
}

/// Applies a structural refactoring action to all matching elements in `doc`.
/// Returns the number of transformations applied.
pub fn apply_structural_action(doc: &mut Document, action: &StructuralAction) -> usize {
    let mut count = 0;
    for_each_element_mut(doc, |el| match action {
        StructuralAction::RenameTag { from, to } => {
            let kind = classify_lenient(el);
            if kind.as_str().eq_ignore_ascii_case(from) {
                match &mut el.sigil {
                    // Renaming replaces the local half and leaves any
                    // namespace in place: `deck.bookmark` renamed to
                    // `card` becomes `deck.card`, not `card`.
                    Sigil::Block(name) => name.name = to.clone(),
                    Sigil::Inline(Some(name)) => name.name = to.clone(),
                    _ => {}
                }
                count += 1;
            }
        }
        StructuralAction::RenameKey { old_key, new_key } => {
            if el.rename_prop_key(old_key, new_key) {
                count += 1;
            }
        }
        StructuralAction::ReplaceValue { key, new_value } => {
            if el.replace_prop_value(key, Value::String(new_value.clone())) {
                count += 1;
            }
        }
    });
    count
}

fn value_contains_str(v: &Value, sub: &str) -> bool {
    match v {
        Value::String(s) => s.contains(sub),
        Value::Map(entries) => entries
            .iter()
            .any(|(k, val)| k.contains(sub) || value_contains_str(val, sub)),
        Value::Seq(items) => items.iter().any(|item| value_contains_str(item, sub)),
        _ => false,
    }
}
