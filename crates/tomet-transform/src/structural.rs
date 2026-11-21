//! Structural in-place AST transformations.

pub use tomet_query::structural::{
    StructuralMatch, StructuralQuery, count_structural_matches, matches_query,
};

use tomet_ast::{Data, File, Name, Value};
use tomet_scalar::Scalar;
use tomet_semantics::Bindings;
use tomet_tree::{DataExt, for_each_declaration_mut, for_each_element_mut};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralAction {
    RenameTag { from: String, to: String },
    RenameKey { old_key: String, new_key: String },
    ReplaceValue { key: String, new_value: String },
}

/// Applies a structural refactoring action to all matching elements in `doc`.
/// Returns the number of transformations applied.
///
/// A kind is what a name resolves to against `bindings` (the names `doc` has in scope, as whoever
/// loaded it bound them): `RenameTag { from: "node-graph.node", .. }` renames `@ng.node` under
/// `::use(node-graph, as: ng)`, and leaves the alias the document writes in place.
pub fn apply_structural_action(
    doc: &mut File,
    bindings: &Bindings,
    action: &StructuralAction,
) -> usize {
    let mut count = 0;
    for_each_element_mut(doc, |el| {
        let kind = bindings.classify_lenient(&el.name);
        count += apply_to(action, kind.as_str(), Some(&mut el.name), &mut el.data);
    });
    for_each_declaration_mut(doc, |declaration| {
        let kind = bindings.classify_attribute_lenient(&declaration.name);
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
        StructuralAction::ReplaceValue { key, new_value } => usize::from(
            data.replace_prop_value(key, Value::Scalar(Scalar::Text(new_value.clone()))),
        ),
    }
}
