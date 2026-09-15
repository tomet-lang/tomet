//! Structural in-place AST transformations.

pub use tomet_search::structural::{
    StructuralMatch, StructuralQuery, count_structural_matches, matches_query,
};

use tomet_ast::{Document, Sigil, Value};
use tomet_semantics::classify_std_lenient;
use tomet_tree::{ElementExt, for_each_element_mut};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralAction {
    RenameTag { from: String, to: String },
    RenameKey { old_key: String, new_key: String },
    ReplaceValue { key: String, new_value: String },
}

/// Applies a structural refactoring action to all matching elements in `doc`.
/// Returns the number of transformations applied.
pub fn apply_structural_action(doc: &mut Document, action: &StructuralAction) -> usize {
    let mut count = 0;
    for_each_element_mut(doc, |el| match action {
        StructuralAction::RenameTag { from, to } => {
            let kind = classify_std_lenient(el);
            if kind.as_str().eq_ignore_ascii_case(from) {
                // Renaming replaces the local half and leaves any
                // namespace in place: `deck.bookmark` renamed to `card`
                // becomes `deck.card`, not `card`.
                if let Sigil::Named(name) = &mut el.sigil {
                    name.name = to.clone();
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
