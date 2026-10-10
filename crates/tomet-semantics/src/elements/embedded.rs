//! An element's value as data.
//!
//! `{...}` is the only data syntax tove has; `element_data` is the single
//! place every caller reads an element's `{...}` group through, so none of
//! them need to know `ElementValue::Group`'s shape themselves.

/// An element's value as data: a `{...}` group's `key: value` pairs, or
/// `None` if the element has no value group at all (or an interpolated
/// one, `${...}`, which is not data).
pub fn element_data(el: &tomet_ast::Element) -> Option<tomet_ast::Value> {
    let data = match el.value.as_ref()? {
        tomet_ast::ElementValue::Group(_) => el.value.as_ref()?.as_data(),
        tomet_ast::ElementValue::Interp(_) => None,
    }?;
    Some(crate::normalize::normalize_data_value(data))
}
