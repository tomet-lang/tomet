//! Helpers for list-shaped elements (`Element::list`, `Element{ sigil:
//! Type("ol"|"ul"), .. }`, see [`crate::classify`]) and their items
//! (`Element::list_item`, `Element{ sigil: Bare, .. }`, nested inside the
//! list's `ElementValue::Children`). Centralizes the "ol"/"ul" ordered
//! check and the `Children` unwrap that `typedmark-codegen-html`/
//! `typedmark-codegen-markdown`/`typedmark-emit-printer` would otherwise
//! each duplicate.

use typedmark_ast::{Element, ElementValue};

use crate::kind::ElementKind;

/// `true` for a `-.` (auto-numbered) list, `false` for a plain `-` list.
/// `None` for any non-list element.
pub fn list_ordered(el: &Element) -> Option<bool> {
    match crate::classify(el) {
        ElementKind::OrderedList => Some(true),
        ElementKind::UnorderedList => Some(false),
        _ => None,
    }
}

/// A list element's items, in source order. Empty for any non-list
/// element, or for a list with no `ElementValue::Children` (shouldn't
/// happen for a parser-produced list, but nothing here assumes it can't).
pub fn list_items(el: &Element) -> &[Element] {
    if list_ordered(el).is_none() {
        return &[];
    }
    match &el.value {
        Some(ElementValue::Children(items)) => items,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_ast::{Sigil, Value};

    #[test]
    fn ordered_list_classifies_and_reports_ordered() {
        let el = Element::list(true, Vec::new(), typedmark_ast::Span::dummy());
        assert_eq!(crate::classify(&el), ElementKind::OrderedList);
        assert_eq!(list_ordered(&el), Some(true));
    }

    #[test]
    fn unordered_list_classifies_and_reports_unordered() {
        let el = Element::list(false, Vec::new(), typedmark_ast::Span::dummy());
        assert_eq!(crate::classify(&el), ElementKind::UnorderedList);
        assert_eq!(list_ordered(&el), Some(false));
    }

    #[test]
    fn non_list_element_has_no_ordered_and_no_items() {
        let el = Element::new(Sigil::Type("codeblock".to_string()));
        assert_eq!(list_ordered(&el), None);
        assert_eq!(list_items(&el), &[] as &[Element]);
    }

    #[test]
    fn list_items_returns_children_in_order() {
        let item = Element::list_item(
            vec![],
            Some(Value::String("marker".into())),
            None,
            Vec::new(),
            typedmark_ast::Span::dummy(),
        );
        let el = Element::list(false, vec![item.clone()], typedmark_ast::Span::dummy());
        assert_eq!(list_items(&el), &[item]);
    }
}
