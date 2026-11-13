//! Giving positional arguments their names.
//!
//! The reader leaves a positional argument unnamed (`name: None` in a [`Tuple`]);
//! what the first, the second... are called is a signature, and the signature belongs to whoever
//! owns the element: a declaration's to `tomet-declaration`, a body element's to `tomet-semantics`.
//! The binding itself is the same for all of them.

use tomet_ast::Tuple;

/// Names the positional arguments of `(args)` after `keys`, in order.
///
/// The reader keeps a positional argument as it was written: an unnamed entry in a [`Tuple`]. A
/// signature says what the first, the second... are called; this puts the names on.
///
/// Over one tuple, each remaining unnamed entry takes the next slot in `keys` that is not already
/// present as a name, left to right on both sides. A slot with no unnamed entry left, and an
/// unnamed entry past the last slot, are both left as they are -- nothing is dropped, and an
/// unassigned entry stays unnamed.
///
/// With no `keys` -- an element that has no positional slots -- `args` comes back unchanged.
///
/// This is the mechanism only; the signatures are the callers'.
pub fn bind_positional(args: &Tuple, keys: &[&str]) -> Tuple {
    if keys.is_empty() {
        return args.clone();
    }
    let mut tuple = args.clone();
    for key in keys {
        if tuple.get(key).is_some() {
            continue;
        }
        if let Some(entry) = tuple.entries_mut().find(|e| e.name().is_none()) {
            entry.set_name(*key);
        }
    }
    tuple
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{TupleEntry, Value};

    fn text(s: &str) -> Value {
        Value::String(s.to_string())
    }

    #[test]
    fn a_lone_scalar_takes_the_first_slot() {
        let args = Tuple::from_unnamed([text("rust")]);
        assert_eq!(
            bind_positional(&args, &["lang"]),
            Tuple::from_named([("lang".into(), text("rust"))])
        );
    }

    #[test]
    fn positional_entries_take_the_slots_in_the_order_written() {
        let args = Tuple::from_unnamed([text("config"), Value::Int(2)]);
        assert_eq!(
            bind_positional(&args, &["kind", "version"]),
            Tuple::from_named([
                ("kind".into(), text("config")),
                ("version".into(), Value::Int(2))
            ])
        );
    }

    #[test]
    fn a_slot_a_real_key_already_fills_is_skipped() {
        // `::kind(config, version: 2)`
        let args = Tuple::from_entries([
            TupleEntry::unnamed(text("config")),
            TupleEntry::named("version", Value::Int(2)),
        ]);
        assert_eq!(
            bind_positional(&args, &["kind", "version"]),
            Tuple::from_named([
                ("kind".into(), text("config")),
                ("version".into(), Value::Int(2))
            ])
        );
    }

    #[test]
    fn nothing_is_dropped_from_a_tuple() {
        // more positional entries than slots: the extra stays unnamed
        let args = Tuple::from_unnamed([text("a"), text("b"), text("c")]);
        let bound = bind_positional(&args, &["x", "y"]);
        assert_eq!(
            bound,
            Tuple::from_entries([
                TupleEntry::named("x", text("a")),
                TupleEntry::named("y", text("b")),
                TupleEntry::unnamed(text("c")),
            ])
        );
    }

    #[test]
    fn with_no_slots_the_arguments_are_as_they_were() {
        let args = Tuple::from_entries([
            TupleEntry::unnamed(text("a")),
            TupleEntry::named("k", Value::Int(1)),
        ]);
        assert_eq!(bind_positional(&args, &[]), args);
    }
}
