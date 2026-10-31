//! An element's value as data.
//!
//! `{...}` is the only data syntax cotomet has; `element_data` is the single
//! place every caller reads an element's `{...}` group through, so none of
//! them need to know `ElementValue::Group`'s shape themselves.

/// An element's value as data: a `{...}` group's `key: value` pairs, or
/// `None` if the element has no value group at all.
pub fn element_data(el: &tomet_ast::Element) -> Option<tomet_ast::Value> {
    el.value.as_ref().map(|value| value.as_data())
}
