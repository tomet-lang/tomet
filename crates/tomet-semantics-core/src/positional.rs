//! Giving positional arguments their names.
//!
//! The reader leaves a positional argument unnamed: its key is its position. What the first, the
//! second... are called is a signature, and the signature belongs to whoever owns the element: a
//! declaration's to `tomet-declaration`, a body element's to `tomet-semantics`. The binding itself
//! is the same for all of them.

use tomet_ast::{Tuple, TupleEntry};

/// Names the unnamed entries of `(args)` after `keys`, in order.
///
/// Each unnamed entry takes, left to right, the next name of `keys` that no entry of `args` has
/// yet. An unnamed entry past the last name stays unnamed, and so keeps a position: the first one
/// left is position 0. A name with no unnamed entry left for it is not added. Nothing is dropped.
///
/// With no `keys` -- an element that has no positional slots -- `args` comes back unchanged.
///
/// This is the mechanism only; the signatures are the callers'.
pub fn bind_positional(args: &Tuple, keys: &[&str]) -> Tuple {
    let mut free = keys.iter().filter(|key| !args.contains_key(key));
    let bound = args.entries().map(|entry| {
        let name = match entry.name() {
            Some(_) => None,
            None => free.next(),
        };
        match name {
            Some(name) => TupleEntry::named(*name, entry.value().clone()),
            None => entry.clone(),
        }
    });
    // A name is given only if no entry has it, and each at most once; the entries left unnamed
    // only move to lower positions, which no name of `args` could be.
    Tuple::try_from_entries(bound).unwrap_or_else(|duplicate| {
        panic!("the signature {keys:?} names two positional arguments alike: {duplicate}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Value;

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
