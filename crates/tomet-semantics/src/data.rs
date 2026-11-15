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

/// [`element_data`] for a declaration: its `{...}` body. Only the header of `::blueprint` has one
/// (see [`tomet_declaration::Shape`]); the other standard declarations are read through
/// [`declaration_entries`].
pub fn declaration_data(declaration: &tomet_ast::Declaration) -> Option<tomet_ast::Value> {
    data_of(&declaration.data)
}

/// The `name: value` entries a declaration is written with, as a tuple: `::meta(title: T)` is
/// `( title: T )`, `::meta()` is the empty tuple, and the dotted head `::field.author(type: string)` is
/// `( author: ( type: string ) )`, the same as `::field(author: ( type: string ))`. `None` when the
/// declaration is not written as entries, such as the lone value of `::meta(x)`.
///
/// What the head, the parentheses and a trailing `[...]` come to is
/// [`tomet_declaration::Declaration::entries`]'s to say; this only asks it.
pub fn declaration_entries(declaration: &tomet_ast::Declaration) -> Option<tomet_ast::Value> {
    tomet_declaration::Declaration::from_ast(declaration)?
        .entries()
        .ok()
        .map(|entries| tomet_ast::Value::Tuple(tomet_ast::Tuple::from_named(entries)))
}

fn data_of(data: &tomet_ast::Data) -> Option<tomet_ast::Value> {
    data.value
        .as_ref()
        .map(|value| tomet_ast::Value::Tuple(value.as_data()))
}
