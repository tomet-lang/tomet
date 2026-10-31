//! Structural in-place AST transformations.

pub use tomet_search::structural::{
    StructuralMatch, StructuralQuery, count_structural_matches, matches_query,
};

use tomet_ast::{Data, Document, Name, Sigil, Value};
use tomet_semantics::{classify_declaration_lenient, classify_std_lenient};
use tomet_tree::{DataExt, for_each_element_mut, for_each_top_level_declaration_mut};

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
    for_each_element_mut(doc, |el| {
        let kind = classify_std_lenient(el);
        let name = match &mut el.sigil {
            Sigil::Named(name) => Some(name),
            _ => None,
        };
        count += apply_to(action, kind.as_str(), name, &mut el.data);
    });
    for_each_top_level_declaration_mut(doc, |declaration| {
        let kind = classify_declaration_lenient(declaration);
        count += apply_to(
            action,
            kind.as_str(),
            Some(&mut declaration.name),
            &mut declaration.data,
        );
    });
    count
}

/// `action` applied to one node: its `kind` (for `RenameTag`), its `name` if
/// it has one to rename, and its `data`. Returns how many transformations it
/// made (0 or 1).
fn apply_to(
    action: &StructuralAction,
    kind: &str,
    name: Option<&mut Name>,
    data: &mut Data,
) -> usize {
    match action {
        StructuralAction::RenameTag { from, to } => {
            if kind.eq_ignore_ascii_case(from) {
                // Renaming replaces the local half and leaves any
                // namespace in place: `deck.bookmark` renamed to `card`
                // becomes `deck.card`, not `card`.
                if let Some(name) = name {
                    name.name = to.clone();
                }
                1
            } else {
                0
            }
        }
        StructuralAction::RenameKey { old_key, new_key } => {
            usize::from(data.rename_prop_key(old_key, new_key))
        }
        StructuralAction::ReplaceValue { key, new_value } => {
            usize::from(data.replace_prop_value(key, Value::String(new_value.clone())))
        }
    }
}
