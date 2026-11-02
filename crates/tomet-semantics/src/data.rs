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

/// [`element_data`] for a declaration.
pub fn declaration_data(declaration: &tomet_ast::Declaration) -> Option<tomet_ast::Value> {
    data_of(&declaration.data)
}

fn data_of(data: &tomet_ast::Data) -> Option<tomet_ast::Value> {
    data.value.as_ref().map(|value| value.as_data())
}
