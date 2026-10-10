//! AST tree normalization and path-based lookup utilities for configuration.

use tomet_ast::Value;
use tomet_tree::ValueExt;

/// Traverses nested `Value::Map` nodes along `path`, returning the referenced value if found.
pub fn get_path<'a>(mut current: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for &segment in path {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Reads a declared list of paths. A single string is accepted as a
/// one-element list shorthand.
pub fn collect_paths(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Seq(items) => {
            for item in items {
                if let Some(path) = item.as_str()
                    && !out.iter().any(|p| p == path)
                {
                    out.push(path.to_string());
                }
            }
        }
        other => {
            if let Some(path) = other.as_str()
                && !out.iter().any(|p| p == path)
            {
                out.push(path.to_string());
            }
        }
    }
}

/// Retrieves or creates an empty `Value::Map` entry at `key` in `target`.
fn get_or_insert_map<'a>(
    target: &'a mut Vec<(String, Value)>,
    key: &str,
) -> &'a mut Vec<(String, Value)> {
    if let Some(pos) = target.iter().position(|(k, _)| k == key) {
        if !matches!(target[pos].1, Value::Map(_)) {
            target[pos].1 = Value::Map(Vec::new());
        }
    } else {
        target.push((key.to_string(), Value::Map(Vec::new())));
    }
    let pos = target.iter().position(|(k, _)| k == key).unwrap();
    match &mut target[pos].1 {
        Value::Map(m) => m,
        _ => unreachable!(),
    }
}

/// Inserts or merges properties into `map` at `key`.
fn insert_or_merge_prop(map: &mut Vec<(String, Value)>, key: &str, value: &Value) {
    if let Some((_, existing)) = map.iter_mut().find(|(k, _)| k == key) {
        match (existing, value) {
            (Value::Map(ex_props), Value::Map(new_props)) => {
                for (pk, pv) in new_props {
                    if let Some((_, p)) = ex_props.iter_mut().find(|(k, _)| k == pk) {
                        *p = pv.clone();
                    } else {
                        ex_props.push((pk.clone(), pv.clone()));
                    }
                }
            }
            (ex, val) => {
                *ex = val.clone();
            }
        }
    } else {
        map.push((key.to_string(), value.clone()));
    }
}

/// Recursively merges `key` and `value` into `target`, normalizing dotted keys
/// into hierarchical `Value::Map` entries while preserving flat keys for `meta` and `macros`.
pub fn merge_entry(target: &mut Vec<(String, Value)>, key: &str, value: &Value) {
    if let Some((head, tail)) = key.split_once('.') {
        if head == "meta" || head == "macros" || head == "macro" {
            // For meta and macros, `tail` is a flat key (e.g. field name or macro name)
            // that should not be split further.
            let sub_map = get_or_insert_map(target, head);
            insert_or_merge_prop(sub_map, tail, value);
            return;
        }

        // General dotted key: descend or create sub-map at `head`.
        let sub_map = get_or_insert_map(target, head);
        merge_entry(sub_map, tail, value);
    } else if let Value::Map(incoming_map) = value {
        let existing_map = get_or_insert_map(target, key);
        if key == "meta" || key == "macros" || key == "macro" {
            for (sub_k, sub_v) in incoming_map {
                insert_or_merge_prop(existing_map, sub_k, sub_v);
            }
        } else {
            for (sub_k, sub_v) in incoming_map {
                merge_entry(existing_map, sub_k, sub_v);
            }
        }
    } else if let Some((_, existing)) = target.iter_mut().find(|(k, _)| k == key) {
        *existing = value.clone();
    } else {
        target.push((key.to_string(), value.clone()));
    }
}
