use tomet_ast::Value;

/// Merges connected `(args)` or `{value}` data into a target's direct `Value`
/// according to the precedence rules in `docs/reviews/2026-08-19-connect-syntax-spec.md`:
///
/// 1. Direct scalar/map properties always take precedence over connected values (Direct > Connected).
/// 2. `Value::Seq` arrays are concatenated (merged).
/// 3. `Value::Map` entries are recursively merged with direct keys overriding connected keys.
pub fn merge_connected_values(direct: Option<&Value>, connected: Option<&Value>) -> Option<Value> {
    match (direct, connected) {
        (None, None) => None,
        (Some(d), None) => Some(d.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(d), Some(c)) => Some(merge_values_inner(d, c)),
    }
}

fn merge_values_inner(direct: &Value, connected: &Value) -> Value {
    match (direct, connected) {
        (Value::Map(d_map), Value::Map(c_map)) => {
            let mut merged = c_map.clone();
            for (d_k, d_v) in d_map {
                if let Some(pos) = merged.iter().position(|(c_k, _)| c_k == d_k) {
                    let c_v = &merged[pos].1;
                    merged[pos].1 = merge_values_inner(d_v, c_v);
                } else {
                    merged.push((d_k.clone(), d_v.clone()));
                }
            }
            Value::Map(merged)
        }
        (Value::Seq(d_seq), Value::Seq(c_seq)) => {
            let mut merged = c_seq.clone();
            for item in d_seq {
                if !merged.contains(item) {
                    merged.push(item.clone());
                }
            }
            Value::Seq(merged)
        }
        // Direct value wins over connected value for scalar/mismatched types
        (direct_val, _connected_val) => direct_val.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_scalar_overrides_connected_scalar() {
        let direct = Value::String("low".to_string());
        let connected = Value::String("high".to_string());
        assert_eq!(
            merge_connected_values(Some(&direct), Some(&connected)),
            Some(Value::String("low".to_string()))
        );
    }

    #[test]
    fn connected_supplies_missing_map_keys() {
        let direct = Value::Map(vec![("tag".to_string(), Value::String("dev".to_string()))]);
        let connected = Value::Map(vec![
            ("priority".to_string(), Value::String("high".to_string())),
            ("tag".to_string(), Value::String("prod".to_string())),
        ]);
        assert_eq!(
            merge_connected_values(Some(&direct), Some(&connected)),
            Some(Value::Map(vec![
                ("priority".to_string(), Value::String("high".to_string())),
                ("tag".to_string(), Value::String("dev".to_string())),
            ]))
        );
    }

    #[test]
    fn merges_sequences() {
        let direct = Value::Seq(vec![Value::String("a".to_string())]);
        let connected = Value::Seq(vec![Value::String("b".to_string())]);
        assert_eq!(
            merge_connected_values(Some(&direct), Some(&connected)),
            Some(Value::Seq(vec![
                Value::String("b".to_string()),
                Value::String("a".to_string()),
            ]))
        );
    }
}
