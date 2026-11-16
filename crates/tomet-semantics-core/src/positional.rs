//! Viewing a tuple through a signature.
//!
//! A tuple holds entries in written order: each either has an explicit name, or is unnamed
//! with its position as its key. What the first, second... unnamed entries mean in a given
//! context is a signature (positional slot names).
//!
//! Binding a signature does not rewrite the tuple: the position remains the address, and
//! the slot name is an alias that answers "which entry does this name mean" without altering
//! the underlying tree, its iteration, JSON serialization, or equality.

use std::borrow::Cow;
use tomet_ast::{Tuple, Value};

/// Positional slot names for a signature, either borrowed as a static/scoped slice or owned.
#[derive(Debug, Clone, PartialEq)]
pub enum Slots<'s> {
    Borrowed(&'s [&'s str]),
    Owned(Vec<String>),
}

impl<'s> Slots<'s> {
    pub fn len(&self) -> usize {
        match self {
            Slots::Borrowed(b) => b.len(),
            Slots::Owned(o) => o.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Option<&str> {
        match self {
            Slots::Borrowed(b) => b.get(index).copied(),
            Slots::Owned(o) => o.get(index).map(String::as_str),
        }
    }
}

/// A view over a [`Tuple`] and a signature's positional slots.
///
/// An unnamed entry takes, left to right, the next slot name that no entry in the tuple
/// already has as an explicit name. An unnamed entry past the last slot has no alias,
/// retaining its decimal position.
#[derive(Debug, Clone, PartialEq)]
pub struct TupleView<'a, 's, V: Clone = Value> {
    tuple: Cow<'a, Tuple<V>>,
    slots: Slots<'s>,
}

impl<'a, 's, V: Clone> TupleView<'a, 's, V> {
    /// Creates a view over a borrowed tuple and borrowed slot names.
    pub fn new(tuple: &'a Tuple<V>, slots: &'s [&'s str]) -> Self {
        Self {
            tuple: Cow::Borrowed(tuple),
            slots: Slots::Borrowed(slots),
        }
    }

    /// Creates a view over a borrowed tuple and owned slot names.
    pub fn with_owned_slots(tuple: &'a Tuple<V>, slots: Vec<String>) -> Self {
        Self {
            tuple: Cow::Borrowed(tuple),
            slots: Slots::Owned(slots),
        }
    }

    /// Creates a view over an owned tuple and borrowed slot names.
    pub fn from_owned(tuple: Tuple<V>, slots: &'s [&'s str]) -> Self {
        Self {
            tuple: Cow::Owned(tuple),
            slots: Slots::Borrowed(slots),
        }
    }

    /// Creates a view over an owned tuple and owned slot names.
    pub fn from_owned_with_owned_slots(tuple: Tuple<V>, slots: Vec<String>) -> Self {
        Self {
            tuple: Cow::Owned(tuple),
            slots: Slots::Owned(slots),
        }
    }

    /// The underlying tuple.
    pub fn tuple(&self) -> &Tuple<V> {
        self.tuple.as_ref()
    }

    /// The slot names of the signature.
    pub fn slots(&self) -> &Slots<'s> {
        &self.slots
    }

    /// Resolves `key` to an entry index in `self.tuple.entries()`:
    /// 1. An entry explicitly named `key`.
    /// 2. An unnamed entry aliased by the slot name `key`.
    /// 3. A decimal position or key matching `self.tuple.index_of(key)`.
    pub fn index_of(&self, key: &str) -> Option<usize> {
        let tuple = self.tuple();
        // 1. Explicitly named entry matching `key`.
        if let Some(idx) = tuple.entries().position(|e| e.name() == Some(key)) {
            return Some(idx);
        }

        // 2. Signature slot matching `key`.
        let mut free_slot_idx = 0;
        for i in 0..self.slots.len() {
            let slot = self.slots.get(i).unwrap();
            // Skip slots that are already explicitly named.
            if tuple.entries().any(|e| e.name() == Some(slot)) {
                continue;
            }
            if slot == key {
                let mut seen_unnamed = 0;
                for (idx, entry) in tuple.entries().enumerate() {
                    if entry.name().is_none() {
                        if seen_unnamed == free_slot_idx {
                            return Some(idx);
                        }
                        seen_unnamed += 1;
                    }
                }
                return None;
            }
            free_slot_idx += 1;
        }

        // 3. Literal key (e.g. decimal position "0", "1").
        tuple.index_of(key)
    }

    /// The value of the entry whose key or slot alias is `key`.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.index_of(key)
            .and_then(|idx| self.tuple().nth(idx).map(|e| e.value()))
    }

    /// Whether an entry is reached by `key` (as an explicit name, a slot alias, or a position).
    pub fn contains_key(&self, key: &str) -> bool {
        self.index_of(key).is_some()
    }

    /// The slot alias assigned to the `unnamed_index`-th unnamed entry, if any.
    pub fn alias_of_unnamed(&self, unnamed_index: usize) -> Option<&str> {
        let mut free_slot_idx = 0;
        for i in 0..self.slots.len() {
            let slot = self.slots.get(i).unwrap();
            if self.tuple().entries().any(|e| e.name() == Some(slot)) {
                continue;
            }
            if free_slot_idx == unnamed_index {
                return Some(slot);
            }
            free_slot_idx += 1;
        }
        None
    }

    /// The slot alias assigned to the entry at written `entry_index`, if it is an unnamed entry
    /// that takes a slot.
    pub fn entry_alias(&self, entry_index: usize) -> Option<&str> {
        let entry = self.tuple().nth(entry_index)?;
        if entry.name().is_some() {
            return None;
        }
        let unnamed_index = self
            .tuple()
            .entries()
            .take(entry_index)
            .filter(|e| e.name().is_none())
            .count();
        self.alias_of_unnamed(unnamed_index)
    }

    /// The name as written, or the slot alias if unnamed, for the entry at written `entry_index`.
    pub fn entry_name_or_alias(&self, entry_index: usize) -> Option<&str> {
        let entry = self.tuple().nth(entry_index)?;
        entry.name().or_else(|| self.entry_alias(entry_index))
    }

    /// Number of unnamed entries that did not receive a slot alias.
    pub fn unaliased_unnamed_count(&self) -> usize {
        let unnamed_count = self
            .tuple()
            .entries()
            .filter(|e| e.name().is_none())
            .count();
        let free_slots_count = (0..self.slots.len())
            .filter_map(|i| self.slots.get(i))
            .filter(|&s| !self.tuple().entries().any(|e| e.name() == Some(s)))
            .count();
        unnamed_count.saturating_sub(free_slots_count)
    }

    /// Whether any unnamed entry did not receive a slot alias.
    pub fn has_unaliased_unnamed(&self) -> bool {
        self.unaliased_unnamed_count() > 0
    }
}

impl<'a, 's, V: Clone> std::ops::Deref for TupleView<'a, 's, V> {
    type Target = Tuple<V>;

    fn deref(&self) -> &Self::Target {
        self.tuple.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{TupleEntry, Value};
    use tomet_scalar::Scalar;

    fn text(s: &str) -> Value {
        Value::Scalar(Scalar::Text(s.to_string()))
    }

    #[test]
    fn a_lone_scalar_takes_the_first_slot_as_alias() {
        let args = Tuple::from_unnamed([text("rust")]);
        let view = TupleView::new(&args, &["lang"]);
        assert_eq!(view.get("lang"), Some(&text("rust")));
        assert_eq!(view.get("0"), Some(&text("rust")));
        assert_eq!(view.entry_alias(0), Some("lang"));
        assert!(!view.has_unaliased_unnamed());
        // The tuple is unchanged:
        assert_eq!(view.tuple(), &args);
    }

    #[test]
    fn positional_entries_take_the_slots_in_the_order_written() {
        let args = Tuple::from_unnamed([text("config"), Value::Scalar(Scalar::Int(2))]);
        let view = TupleView::new(&args, &["kind", "version"]);
        assert_eq!(view.get("kind"), Some(&text("config")));
        assert_eq!(view.get("0"), Some(&text("config")));
        assert_eq!(view.get("version"), Some(&Value::Scalar(Scalar::Int(2))));
        assert_eq!(view.get("1"), Some(&Value::Scalar(Scalar::Int(2))));
        assert_eq!(view.entry_alias(0), Some("kind"));
        assert_eq!(view.entry_alias(1), Some("version"));
        assert!(!view.has_unaliased_unnamed());
    }

    #[test]
    fn a_slot_a_real_key_already_fills_is_skipped() {
        // `::kind(config, version: 2)`
        let args = Tuple::from_entries([
            TupleEntry::unnamed(text("config")),
            TupleEntry::named("version", Value::Scalar(Scalar::Int(2))),
        ]);
        let view = TupleView::new(&args, &["kind", "version"]);
        assert_eq!(view.get("kind"), Some(&text("config")));
        assert_eq!(view.get("0"), Some(&text("config")));
        assert_eq!(view.get("version"), Some(&Value::Scalar(Scalar::Int(2))));
        assert_eq!(view.get("1"), None);
        assert_eq!(view.entry_alias(0), Some("kind"));
        assert_eq!(view.entry_alias(1), None);
        assert!(!view.has_unaliased_unnamed());
    }

    #[test]
    fn extra_positional_entries_past_the_last_slot_have_no_alias() {
        // More positional entries than slots: the extra stays unnamed without alias
        let args = Tuple::from_unnamed([text("a"), text("b"), text("c")]);
        let view = TupleView::new(&args, &["x", "y"]);
        assert_eq!(view.get("x"), Some(&text("a")));
        assert_eq!(view.get("0"), Some(&text("a")));
        assert_eq!(view.get("y"), Some(&text("b")));
        assert_eq!(view.get("1"), Some(&text("b")));
        assert_eq!(view.get("2"), Some(&text("c")));
        assert_eq!(view.get("z"), None);
        assert_eq!(view.entry_alias(0), Some("x"));
        assert_eq!(view.entry_alias(1), Some("y"));
        assert_eq!(view.entry_alias(2), None);
        assert!(view.has_unaliased_unnamed());
        assert_eq!(view.unaliased_unnamed_count(), 1);
    }

    #[test]
    fn with_no_slots_all_unnamed_entries_are_unaliased() {
        let args = Tuple::from_entries([
            TupleEntry::unnamed(text("a")),
            TupleEntry::named("k", Value::Scalar(Scalar::Int(1))),
        ]);
        let view = TupleView::new(&args, &[]);
        assert_eq!(view.get("0"), Some(&text("a")));
        assert_eq!(view.get("k"), Some(&Value::Scalar(Scalar::Int(1))));
        assert_eq!(view.entry_alias(0), None);
        assert!(view.has_unaliased_unnamed());
        assert_eq!(view.unaliased_unnamed_count(), 1);
    }
}
