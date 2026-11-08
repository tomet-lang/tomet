//! An element's value as data.
//!
//! `{...}` is the only data syntax cotomet has; `element_data` is the single
//! place every caller reads an element's `{...}` group through, so none of
//! them need to know `Group`'s shape themselves.

/// An element's value as data: a `{...}` group's `key: value` pairs, or
/// `None` if the element has no value group at all.
pub fn element_data(el: tomet_ast::ElementView<'_>) -> Option<tomet_ast::Value> {
    data_of(el.data)
}

/// [`element_data`] for a declaration: its `{...}` body. Only `::vocabulary`, `::blueprint` and
/// `::dictionary` have one (see [`tomet_declaration::Shape`]); the other standard declarations
/// are read through [`declaration_entries`].
pub fn declaration_data(declaration: &tomet_ast::Declaration) -> Option<tomet_ast::Value> {
    data_of(&declaration.data)
}

/// The `name: value` entries a declaration of shape [`tomet_declaration::Shape::Entries`] is written
/// with, as a map: `::meta(title: T)` is `{ title: T }` and `::meta()` is the empty map. `None` when
/// the parentheses hold something else, such as the lone value of `::meta(x)`.
pub fn declaration_entries(declaration: &tomet_ast::Declaration) -> Option<tomet_ast::Value> {
    match &declaration.data.args {
        Some(map @ tomet_ast::Value::Map(_)) => Some(map.clone()),
        _ => None,
    }
}

fn data_of(data: &tomet_ast::Data) -> Option<tomet_ast::Value> {
    data.value.as_ref().map(|value| value.as_data())
}
