//! AST tree normalization and path-based lookup utilities for configuration.

use tomet_ast::{Tuple, Value};
use tomet_tree::ValueExt;

/// Traverses nested `Value::Tuple` nodes along `path`, returning the referenced value if found.
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
        Value::Tuple(tuple) if tuple.has_no_named() => {
            for item in tuple.unnamed() {
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

/// Retrieves or creates an empty `Value::Tuple` (named) entry at `key` in `target`.
fn get_or_insert_map<'a>(target: &'a mut Tuple, key: &str) -> &'a mut Tuple {
    if !target.contains_key(key) {
        target.set(key, Value::Tuple(Tuple::new()));
    }
    let val = target.get_mut(key).unwrap();
    if !matches!(val, Value::Tuple(_)) {
        *val = Value::Tuple(Tuple::new());
    }
    match target.get_mut(key).unwrap() {
        Value::Tuple(m) => m,
        _ => unreachable!(),
    }
}

/// Inserts or merges properties into `tuple` at `key`.
fn insert_or_merge_prop(tuple: &mut Tuple, key: &str, value: &Value) {
    if let Some(existing) = tuple.get_mut(key) {
        match (existing, value) {
            (Value::Tuple(ex_props), Value::Tuple(new_props)) if !new_props.has_no_named() => {
                for (pk, pv) in new_props.named() {
                    ex_props.set(pk, pv.clone());
                }
            }
            (ex, val) => {
                *ex = val.clone();
            }
        }
    } else {
        tuple.set(key, value.clone());
    }
}

/// Recursively merges `key` and `value` into `target`, normalizing dotted keys
/// into hierarchical `Value::Tuple` entries while preserving flat keys for `meta`.
pub fn merge_entry(target: &mut Tuple, key: &str, value: &Value) {
    if let Some((head, tail)) = key.split_once('.') {
        if head == "meta" {
            // For meta, `tail` is a flat key (a field name)
            // that should not be split further.
            let sub_map = get_or_insert_map(target, head);
            insert_or_merge_prop(sub_map, tail, value);
            return;
        }

        // General dotted key: descend or create sub-map at `head`.
        let sub_map = get_or_insert_map(target, head);
        merge_entry(sub_map, tail, value);
    } else if let Value::Tuple(incoming_tuple) = value {
        if !incoming_tuple.has_no_named() {
            let existing_map = get_or_insert_map(target, key);
            if key == "meta" {
                for (sub_k, sub_v) in incoming_tuple.named() {
                    insert_or_merge_prop(existing_map, sub_k, sub_v);
                }
            } else {
                for (sub_k, sub_v) in incoming_tuple.named() {
                    merge_entry(existing_map, sub_k, sub_v);
                }
            }
        } else {
            target.set(key, value.clone());
        }
    } else {
        target.set(key, value.clone());
    }
}
