//! Extracts the `level` of a heading-shaped element (`Heading`, per
//! [`crate::classify`]) from its `args`, centralizing the `1..=6` clamp
//! that `typedmark-codegen-html`/`typedmark-codegen-markdown`/
//! `typedmark-emit-printer` each used to apply separately.

use typedmark_ast::{Element, Value};

use crate::kind::ElementKind;
use crate::positional::normalized_element_args;

/// The clamped `1..=6` heading level of a heading-shaped element, if it
/// has one. `None` for any non-heading element, or for a heading whose
/// `level` argument is missing or not an integer (e.g. hand-authored
/// `@heading(level: "two")`) -- callers fall back to a default level
/// themselves, this function never guesses one.
pub fn heading_level(el: &Element) -> Option<u8> {
    if crate::classify(el) != ElementKind::Heading {
        return None;
    }
    match normalized_element_args(el) {
        Some(Value::Map(entries)) => entries.iter().find_map(|(k, v)| {
            if k != "level" {
                return None;
            }
            match v {
                Value::Int(n) => Some((*n).clamp(1, 6) as u8),
                _ => None,
            }
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typedmark_ast::Sigil;

    #[test]
    fn type_sigil_heading_classifies_and_extracts_level() {
        let mut el = Element::new(Sigil::Type("heading".to_string()));
        el.args = Some(Value::Int(2));
        assert_eq!(crate::classify(&el), ElementKind::Heading);
        assert_eq!(heading_level(&el), Some(2));
    }

    #[test]
    fn at_sigil_heading_classifies_and_extracts_level() {
        let mut el = Element::new(Sigil::At(Some("heading".to_string())));
        el.args = Some(Value::Int(3));
        assert_eq!(crate::classify(&el), ElementKind::Heading);
        assert_eq!(heading_level(&el), Some(3));
    }

    #[test]
    fn builtin_positional_arg_keys_returns_level_for_both_sigil_forms() {
        assert_eq!(
            crate::builtin_positional_arg_keys(&Sigil::Type("heading".to_string())),
            &["level"]
        );
        assert_eq!(
            crate::builtin_positional_arg_keys(&Sigil::At(Some("heading".to_string()))),
            &["level"]
        );
    }

    #[test]
    fn bare_int_arg_normalizes_into_level() {
        // `#[x]` sugar produces a bare `Value::Int(level)` in `args`, not
        // already wrapped in a map -- `normalized_element_args` wraps it
        // under the single "level" positional slot before this reads it.
        let mut el = Element::new(Sigil::At(Some("heading".to_string())));
        el.args = Some(Value::Int(1));
        assert_eq!(heading_level(&el), Some(1));
    }

    #[test]
    fn clamps_level_above_six_down_to_six() {
        let mut el = Element::new(Sigil::Type("heading".to_string()));
        el.args = Some(Value::Int(9));
        assert_eq!(heading_level(&el), Some(6));
    }

    #[test]
    fn clamps_level_below_one_up_to_one() {
        let mut el = Element::new(Sigil::Type("heading".to_string()));
        el.args = Some(Value::Int(0));
        assert_eq!(heading_level(&el), Some(1));
    }

    #[test]
    fn non_heading_element_is_always_none() {
        let mut el = Element::new(Sigil::Type("codeblock".to_string()));
        el.args = Some(Value::Int(2));
        assert_eq!(heading_level(&el), None);
    }

    #[test]
    fn heading_with_missing_level_is_none() {
        let el = Element::new(Sigil::Type("heading".to_string()));
        assert_eq!(heading_level(&el), None);
    }

    #[test]
    fn heading_with_non_int_level_is_none() {
        let mut el = Element::new(Sigil::Type("heading".to_string()));
        el.args = Some(Value::Map(vec![(
            "level".to_string(),
            Value::String("two".to_string()),
        )]));
        assert_eq!(heading_level(&el), None);
    }
}
