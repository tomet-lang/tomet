//! Helpers for list-shaped elements (`Element::list`, `Element{ sigil:
//! Type("ol"|"ul"), .. }`, see [`crate::classify`]) and their items
//! (`Element::list_item`, `Element{ sigil: Bare, .. }`, nested inside the
//! list's `ElementValue::Children`). Centralizes the "ol"/"ul" ordered
//! check and the `Children` unwrap that `tomet-html`/`tomet-markdown`/`tomet-printer` would otherwise
//! each duplicate.

use tomet_ast::Element;

use crate::kind::ElementKind;

/// `true` for a `-.` (auto-numbered) list, `false` for a plain `-` list.
/// `None` for any non-list element.
pub fn list_ordered(el: &Element) -> Option<bool> {
    match crate::classify_lenient(el) {
        ElementKind::OrderedList => Some(true),
        ElementKind::UnorderedList => Some(false),
        _ => None,
    }
}

/// A list element's items, in source order. Empty for any non-list
/// element, or for a list with no `ElementValue::Children` (shouldn't
/// happen for a parser-produced list, but nothing here assumes it can't).
pub fn list_items(el: &Element) -> Vec<&Element> {
    if list_ordered(el).is_none() {
        return Vec::new();
    }
    match &el.value {
        Some(value) => value.as_children(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Sigil, Value};
    use tomet_tree::{element_list, element_list_item, element_new};

    #[test]
    fn ordered_list_classifies_and_reports_ordered() {
        let el = element_list(true, Vec::new(), tomet_ast::Span::dummy());
        assert_eq!(crate::classify_lenient(&el), ElementKind::OrderedList);
        assert_eq!(list_ordered(&el), Some(true));
    }

    #[test]
    fn unordered_list_classifies_and_reports_unordered() {
        let el = element_list(false, Vec::new(), tomet_ast::Span::dummy());
        assert_eq!(crate::classify_lenient(&el), ElementKind::UnorderedList);
        assert_eq!(list_ordered(&el), Some(false));
    }

    #[test]
    fn non_list_element_has_no_ordered_and_no_items() {
        let el = element_new(Sigil::block("codeblock"));
        assert_eq!(list_ordered(&el), None);
        assert!(list_items(&el).is_empty());
    }

    #[test]
    fn list_items_returns_children_in_order() {
        let item = element_list_item(
            vec![],
            Some(Value::String("marker".into())),
            None,
            Vec::new(),
            tomet_ast::Span::dummy(),
        );
        let el = element_list(false, vec![item.clone()], tomet_ast::Span::dummy());
        assert_eq!(list_items(&el), vec![&item]);
    }
}
