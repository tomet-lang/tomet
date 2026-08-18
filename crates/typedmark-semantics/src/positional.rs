use typedmark_ast::{Element, Sigil, Value};

/// Maps official built-in element names to their required/positional argument key.
///
/// Official built-in elements:
/// - `<codeblock>` -> `"lang"`
/// - `<embed>` -> `"src"`
/// - `@meta` / `<meta>` -> `"format"`
/// - `@config` / `<config>` -> `"format"`
///
/// Note: Unnamed `@` elements (e.g. `@(...)`) are deliberately excluded here because
/// `@` inference relies on explicit `key:` names (e.g. `url:`, `file:`, `ref:`).
pub fn builtin_positional_arg_key(sigil: &Sigil) -> Option<&'static str> {
    match sigil {
        Sigil::Type(name) => match name.as_str() {
            "codeblock" => Some("lang"),
            "embed" => Some("src"),
            "meta" | "config" => Some("format"),
            _ => None,
        },
        Sigil::At(Some(name)) => match name.as_str() {
            "meta" | "config" => Some("format"),
            _ => None,
        },
        _ => None,
    }
}

/// Returns the normalized `args` map for an [`Element`].
///
/// If `el.args` contains a single scalar value (e.g. `<codeblock>("rust")`) and `el`
/// is an official built-in element with a positional arg mapping, this converts
/// the positional scalar value into a `Value::Map` containing the inferred key
/// (e.g. `{"lang": "rust"}`).
///
/// If `el.args` is already a `Value::Map` or `el` is not a recognized built-in element
/// with a positional arg key, `el.args` is returned as-is.
pub fn normalized_element_args(el: &Element) -> Option<Value> {
    let args = el.args.as_ref()?;
    match args {
        Value::Map(_) => Some(args.clone()),
        scalar => {
            if let Some(key) = builtin_positional_arg_key(&el.sigil) {
                Some(Value::Map(vec![(key.to_string(), scalar.clone())]))
            } else {
                Some(scalar.clone())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_codeblock_positional_arg() {
        let mut el = Element::new(Sigil::Type("codeblock".to_string()));
        el.args = Some(Value::String("rust".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "lang".to_string(),
                Value::String("rust".to_string())
            )]))
        );
    }

    #[test]
    fn normalizes_embed_positional_arg() {
        let mut el = Element::new(Sigil::Type("embed".to_string()));
        el.args = Some(Value::String("foo.png".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "src".to_string(),
                Value::String("foo.png".to_string())
            )]))
        );
    }

    #[test]
    fn normalizes_meta_positional_arg() {
        let mut el = Element::new(Sigil::At(Some("meta".to_string())));
        el.args = Some(Value::String("json".to_string()));
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::Map(vec![(
                "format".to_string(),
                Value::String("json".to_string())
            )]))
        );
    }

    #[test]
    fn leaves_map_args_unchanged() {
        let mut el = Element::new(Sigil::Type("codeblock".to_string()));
        let map_val = Value::Map(vec![("lang".to_string(), Value::String("rust".to_string()))]);
        el.args = Some(map_val.clone());
        assert_eq!(normalized_element_args(&el), Some(map_val));
    }

    #[test]
    fn unnamed_at_is_never_normalized() {
        let mut el = Element::new(Sigil::At(None));
        el.args = Some(Value::String("https://example.com".to_string()));
        // Unnamed `@` has no positional arg key, so it returns scalar as-is
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::String("https://example.com".to_string()))
        );
    }
}
