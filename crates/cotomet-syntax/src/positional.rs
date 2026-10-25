//! Giving positional arguments their names.
//!
//! The reader leaves a positional argument unnamed (an entry keyed [`POSITIONAL_ENTRY_KEY`]);
//! what the first, the second... are called is a signature, and the signature belongs to whoever
//! owns the element: a declaration's to `tomet-declaration`, a body element's to `tomet-semantics`.
//! The binding itself is the same for all of them, and is here, next to the key it undoes.

use tomet_ast::Value;

use crate::parser::POSITIONAL_ENTRY_KEY;

/// Names the positional arguments of `(args)` after `keys`, in order.
///
/// The reader keeps a positional argument as it was written: an entry whose key is
/// [`POSITIONAL_ENTRY_KEY`] in a map, an item of a sequence, or the lone scalar of `(x)`. A
/// signature says what the first, the second... are called; this puts the names on.
///
/// - A map: each remaining positional entry takes the next slot in `keys` that is not already
///   a real key, left to right on both sides. A slot with no positional entry left, and a positional
///   entry past the last slot, are both left as they are -- nothing is dropped, and an unassigned
///   entry keeps its empty key.
/// - A sequence (`(list(a, b))`): zipped against the slots by position; items past the last slot
///   are dropped.
/// - Any other value: the lone argument, under the first slot.
///
/// With no `keys` -- an element that has no positional slots -- `args` comes back unchanged.
///
/// This is the mechanism only; the signatures are the callers'.
pub fn bind_positional(args: &Value, keys: &[&str]) -> Value {
    let Some(first) = keys.first() else {
        return args.clone();
    };
    match args {
        Value::Map(entries) => {
            let mut entries = entries.clone();
            for key in keys {
                if entries.iter().any(|(k, _)| k == key) {
                    continue;
                }
                if let Some(pos) = entries.iter().position(|(k, _)| k == POSITIONAL_ENTRY_KEY) {
                    entries[pos].0 = (*key).to_string();
                }
            }
            Value::Map(entries)
        }
        Value::Seq(items) => Value::Map(
            items
                .iter()
                .zip(keys)
                .map(|(value, key)| ((*key).to_string(), value.clone()))
                .collect(),
        ),
        scalar => Value::Map(vec![((*first).to_string(), scalar.clone())]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::String(s.to_string())
    }
    fn pos(value: Value) -> (String, Value) {
        (POSITIONAL_ENTRY_KEY.to_string(), value)
    }

    #[test]
    fn a_lone_scalar_takes_the_first_slot() {
        assert_eq!(
            bind_positional(&text("rust"), &["lang"]),
            Value::Map(vec![("lang".into(), text("rust"))])
        );
    }

    #[test]
    fn positional_entries_take_the_slots_in_the_order_written() {
        let args = Value::Map(vec![pos(text("config")), pos(Value::Int(2))]);
        assert_eq!(
            bind_positional(&args, &["kind", "version"]),
            Value::Map(vec![
                ("kind".into(), text("config")),
                ("version".into(), Value::Int(2))
            ])
        );
    }

    #[test]
    fn a_slot_a_real_key_already_fills_is_skipped() {
        // `::kind(config, version: 2)`
        let args = Value::Map(vec![pos(text("config")), ("version".into(), Value::Int(2))]);
        assert_eq!(
            bind_positional(&args, &["kind", "version"]),
            Value::Map(vec![
                ("kind".into(), text("config")),
                ("version".into(), Value::Int(2))
            ])
        );
    }

    #[test]
    fn nothing_is_dropped_from_a_map() {
        // more positional entries than slots: the extra keeps its empty key
        let args = Value::Map(vec![pos(text("a")), pos(text("b")), pos(text("c"))]);
        let bound = bind_positional(&args, &["x", "y"]);
        assert_eq!(
            bound,
            Value::Map(vec![
                ("x".into(), text("a")),
                ("y".into(), text("b")),
                (POSITIONAL_ENTRY_KEY.into(), text("c"))
            ])
        );
    }

    #[test]
    fn a_sequence_is_zipped_against_the_slots() {
        let args = Value::Seq(vec![text("a"), text("b"), text("c")]);
        assert_eq!(
            bind_positional(&args, &["x", "y"]),
            Value::Map(vec![("x".into(), text("a")), ("y".into(), text("b"))])
        );
    }

    #[test]
    fn with_no_slots_the_arguments_are_as_they_were() {
        let args = Value::Map(vec![pos(text("a")), ("k".into(), Value::Int(1))]);
        assert_eq!(bind_positional(&args, &[]), args);
        assert_eq!(bind_positional(&text("x"), &[]), text("x"));
    }
}
