use std::collections::HashMap;
use typedmark_ast::{Element, ElementValue, Sigil, Value};

/// Schema definition extracted from `@settings` block for custom elements.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementSchema {
    /// Ordered list of keys allowed for positional inference.
    pub positional: Vec<String>,
}

/// Project/Document level settings schema parsed from `@settings`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsSchema {
    pub elements: HashMap<String, ElementSchema>,
}

impl SettingsSchema {
    /// Parses a [`SettingsSchema`] from an `@settings` element.
    pub fn from_element(el: &Element) -> Self {
        let is_settings = matches!(&el.sigil, Sigil::At(Some(name)) | Sigil::Type(name) if name == "settings");
        if !is_settings {
            return Self::default();
        }

        let val = match &el.value {
            Some(ElementValue::Data(v)) => v,
            _ => return Self::default(),
        };

        Self::from_value(val)
    }

    /// Parses a [`SettingsSchema`] from a `Value::Map` representing `@settings` data.
    pub fn from_value(val: &Value) -> Self {
        let mut schema = Self::default();
        let Value::Map(root_entries) = val else {
            return schema;
        };

        for (root_k, root_v) in root_entries {
            if root_k == "elements" {
                if let Value::Map(elem_entries) = root_v {
                    for (elem_name, elem_v) in elem_entries {
                        let mut elem_schema = ElementSchema::default();
                        if let Value::Map(props) = elem_v {
                            for (prop_k, prop_v) in props {
                                if prop_k == "positional" {
                                    elem_schema.positional = parse_string_list(prop_v);
                                }
                            }
                        }
                        schema.elements.insert(elem_name.clone(), elem_schema);
                    }
                }
            }
        }
        schema
    }

    /// Returns the positional arg key list for an element name, if defined in schema.
    pub fn positional_keys(&self, elem_name: &str) -> Option<&[String]> {
        self.elements.get(elem_name).map(|s| s.positional.as_slice())
    }
}

fn parse_string_list(val: &Value) -> Vec<String> {
    match val {
        Value::String(s) => vec![s.clone()],
        Value::Seq(seq) => seq
            .iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

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

/// Returns the normalized `args` map for an [`Element`], using default built-in rules.
pub fn normalized_element_args(el: &Element) -> Option<Value> {
    normalized_element_args_with_schema(el, &SettingsSchema::default())
}

/// Returns the normalized `args` map for an [`Element`], taking custom `@settings` into account.
pub fn normalized_element_args_with_schema(el: &Element, schema: &SettingsSchema) -> Option<Value> {
    let args = el.args.as_ref()?;
    let elem_name = match &el.sigil {
        Sigil::Type(name) => Some(name.as_str()),
        Sigil::At(Some(name)) => Some(name.as_str()),
        _ => None,
    };

    if let Value::Map(_) = args {
        return Some(args.clone());
    }

    // 1. Check custom `@settings` schema first
    if let Some(name) = elem_name {
        if let Some(pos_keys) = schema.positional_keys(name) {
            if !pos_keys.is_empty() {
                match args {
                    Value::Seq(seq) => {
                        let mut entries = Vec::new();
                        for (idx, val) in seq.iter().enumerate() {
                            if idx < pos_keys.len() {
                                entries.push((pos_keys[idx].clone(), val.clone()));
                            }
                        }
                        return Some(Value::Map(entries));
                    }
                    scalar => {
                        return Some(Value::Map(vec![(pos_keys[0].clone(), scalar.clone())]));
                    }
                }
            }
        }
    }

    // 2. Fall back to built-in positional key mapping
    if let scalar = args {
        if let Some(key) = builtin_positional_arg_key(&el.sigil) {
            return Some(Value::Map(vec![(key.to_string(), scalar.clone())]));
        }
    }

    Some(args.clone())
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
        assert_eq!(
            normalized_element_args(&el),
            Some(Value::String("https://example.com".to_string()))
        );
    }

    #[test]
    fn normalizes_custom_element_single_positional_arg_via_settings() {
        let settings_val = Value::Map(vec![(
            "elements".to_string(),
            Value::Map(vec![(
                "task".to_string(),
                Value::Map(vec![(
                    "positional".to_string(),
                    Value::Seq(vec![Value::String("title".to_string())]),
                )]),
            )]),
        )]);
        let schema = SettingsSchema::from_value(&settings_val);

        let mut el = Element::new(Sigil::Type("task".to_string()));
        el.args = Some(Value::String("Clean room".to_string()));

        assert_eq!(
            normalized_element_args_with_schema(&el, &schema),
            Some(Value::Map(vec![(
                "title".to_string(),
                Value::String("Clean room".to_string())
            )]))
        );
    }

    #[test]
    fn normalizes_custom_element_multiple_positional_args_seq_via_settings() {
        let settings_val = Value::Map(vec![(
            "elements".to_string(),
            Value::Map(vec![(
                "task".to_string(),
                Value::Map(vec![(
                    "positional".to_string(),
                    Value::Seq(vec![
                        Value::String("title".to_string()),
                        Value::String("priority".to_string()),
                    ]),
                )]),
            )]),
        )]);
        let schema = SettingsSchema::from_value(&settings_val);

        let mut el = Element::new(Sigil::Type("task".to_string()));
        el.args = Some(Value::Seq(vec![
            Value::String("Clean room".to_string()),
            Value::String("high".to_string()),
        ]));

        assert_eq!(
            normalized_element_args_with_schema(&el, &schema),
            Some(Value::Map(vec![
                ("title".to_string(), Value::String("Clean room".to_string())),
                ("priority".to_string(), Value::String("high".to_string())),
            ]))
        );
    }
}
