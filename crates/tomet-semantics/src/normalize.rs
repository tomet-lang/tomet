use tomet_ast::Value;

/// Normalizes a [`Value`] in a data context (e.g. `@meta`, `@config`, or element attributes).
///
/// Specifically, resolves call literals that represent canonical data structures:
/// - `list(a, b, ...)` -> `Value::Seq(vec![a, b, ...])`
///
/// Recursively traverses nested `Value::Seq`, `Value::Map`, and `Value::Call`.
pub fn normalize_data_value(v: Value) -> Value {
    match v {
        Value::Call(name, args) if name == "list" => {
            Value::Seq(args.into_iter().map(normalize_data_value).collect())
        }
        Value::Call(name, args) => {
            Value::Call(name, args.into_iter().map(normalize_data_value).collect())
        }
        Value::Seq(items) => Value::Seq(items.into_iter().map(normalize_data_value).collect()),
        Value::Map(entries) => Value::Map(
            entries
                .into_iter()
                .map(|(k, v)| (k, normalize_data_value(v)))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_top_level_list_call_to_seq() {
        let call = Value::Call(
            "list".to_string(),
            vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
            ],
        );
        assert_eq!(
            normalize_data_value(call),
            Value::Seq(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
            ])
        );
    }

    #[test]
    fn leaves_other_calls_intact() {
        let call = Value::Call("other".to_string(), vec![Value::String("x".to_string())]);
        assert_eq!(normalize_data_value(call.clone()), call);
    }

    #[test]
    fn recursively_normalizes_maps_and_sequences() {
        let nested = Value::Map(vec![(
            "tags".to_string(),
            Value::Call(
                "list".to_string(),
                vec![
                    Value::String("tomet".to_string()),
                    Value::Call("list".to_string(), vec![Value::Int(1), Value::Int(2)]),
                ],
            ),
        )]);

        let expected = Value::Map(vec![(
            "tags".to_string(),
            Value::Seq(vec![
                Value::String("tomet".to_string()),
                Value::Seq(vec![Value::Int(1), Value::Int(2)]),
            ]),
        )]);

        assert_eq!(normalize_data_value(nested), expected);
    }
}
