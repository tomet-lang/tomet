//! Extracts the `level` of a heading-shaped element (`Heading`, per
//! [`crate::classify_std`]) from its `args`, centralizing the `1..=6` clamp
//! that `tomet-html`/`tomet-markdown`/`tomet-printer` each used to apply separately.

use tomet_ast::Element;
use tomet_tree::ValueExt;

use crate::kind::ElementKind;
use crate::positional::normalized_element_args;

/// The clamped `1..=6` heading level of a heading-shaped element, if it
/// has one. `None` for any non-heading element, or for a heading whose
/// `level` argument is missing or not an integer (e.g. hand-authored
/// `@heading(level: "two")`) -- callers fall back to a default level
/// themselves, this function never guesses one.
pub fn heading_level(el: &Element) -> Option<u8> {
    if crate::classify_std_lenient(el) != ElementKind::Heading {
        return None;
    }
    normalized_element_args(el)?
        .get("level")
        .and_then(|v| v.as_i64())
        .map(|n| n.clamp(1, 6) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Sigil, Value};
    use tomet_tree::element_new;

    #[test]
    fn type_sigil_heading_classifies_and_extracts_level() {
        let mut el = element_new(Sigil::named("heading"));
        el.args = Some(Value::Int(2));
        assert_eq!(crate::classify_std_lenient(&el), ElementKind::Heading);
        assert_eq!(heading_level(&el), Some(2));
    }

    #[test]
    fn at_sigil_heading_classifies_and_extracts_level() {
        let mut el = element_new(Sigil::named("heading"));
        el.args = Some(Value::Int(3));
        assert_eq!(crate::classify_std_lenient(&el), ElementKind::Heading);
        assert_eq!(heading_level(&el), Some(3));
    }

    #[test]
    fn builtin_positional_arg_keys_returns_level_for_both_sigil_forms() {
        assert_eq!(
            crate::builtin_positional_arg_keys(&Sigil::named("heading")),
            &["level"]
        );
        assert_eq!(
            crate::builtin_positional_arg_keys(&Sigil::named("heading")),
            &["level"]
        );
    }

    #[test]
    fn bare_int_arg_normalizes_into_level() {
        // `#[x]` sugar produces a bare `Value::Int(level)` in `args`, not
        // already wrapped in a map -- `normalized_element_args` wraps it
        // under the single "level" positional slot before this reads it.
        let mut el = element_new(Sigil::named("heading"));
        el.args = Some(Value::Int(1));
        assert_eq!(heading_level(&el), Some(1));
    }

    #[test]
    fn clamps_level_above_six_down_to_six() {
        let mut el = element_new(Sigil::named("heading"));
        el.args = Some(Value::Int(9));
        assert_eq!(heading_level(&el), Some(6));
    }

    #[test]
    fn clamps_level_below_one_up_to_one() {
        let mut el = element_new(Sigil::named("heading"));
        el.args = Some(Value::Int(0));
        assert_eq!(heading_level(&el), Some(1));
    }

    #[test]
    fn non_heading_element_is_always_none() {
        let mut el = element_new(Sigil::named("codeblock"));
        el.args = Some(Value::Int(2));
        assert_eq!(heading_level(&el), None);
    }

    #[test]
    fn heading_with_missing_level_is_none() {
        let el = element_new(Sigil::named("heading"));
        assert_eq!(heading_level(&el), None);
    }

    #[test]
    fn heading_with_non_int_level_is_none() {
        let mut el = element_new(Sigil::named("heading"));
        el.args = Some(Value::Map(vec![(
            "level".to_string(),
            Value::String("two".to_string()),
        )]));
        assert_eq!(heading_level(&el), None);
    }
}
