//! =[ tomet-ast ]
//!
//! Shared AST and `Value` data models for the Tomet toolchain ([`File`], [`Block`],
//! [`Inline`], [`Element`], [`ElementView`], [`BodyView`], [`Declaration`], [`Interpolation`],
//! [`Hole`], [`TypeSeat`], [`ListItem`], [`Value`], [`Tuple`], [`TupleEntry`], [`Span`]).
//!
//! =[ Design Policy ]
//!
//! - (Pure Data Models & Structural Accessors Only)
//!   | `tomet-ast` is the foundational AST crate referenced across the workspace,
//!   | depending only on `tomet-span`, `serde`, and `unicode-width`.
//!   | It contains only struct/enum definitions and pure inherent methods for construction
//!   | (`new`, `with_*`) and structural inspection (`as_str()`, `get()`, `is_none()`).
//! - (A Node's Role Is Its Type)
//!   | A `{$ ... }`, `$(...)` or `$name(...)` is an [`Interpolation`] (an expression and a span), not an [`Element`]:
//!   | it has no name, no groups and no connects, so it is a variant of [`Block`],
//!   | [`Inline`] and [`Value`] and no walker over elements ever meets one. Reach every
//!   | interpolation of a file through `tomet_tree::for_each_interpolation`, and every type seat
//!   | through `tomet_tree::for_each_type_seat`.
//!   | A `::name(...)` is a [`Declaration`]: sits in the file preamble header ([`File::header`]),
//!   | about the file and attached to nothing, so it has no placement and takes no connect.
//!   | A `::name(...)` is a [`Connect`]: metadata about its owner, with no id, no connects of its own
//!   | and no placement. A `<?matcher(...)>` is a [`Hole`] (a matcher, optional, in place of a name).
//!   | A `<$type(...)>` is a [`TypeSeat`]: the type as the expression it is written (`uuid`,
//!   | `list(string, length: 6)`), and nothing else; it is not a [`Hole`], which has a matcher and
//!   | groups, and not an [`Interpolation`], which is evaluated where it stands. `tomet-ast` never
//!   | says which type names are valid, nor what `auto` in `default: auto` is: those are
//!   | `tomet-type`'s and the semantics'.
//!   | `<...>` is one family in the *syntax* (the CST has one `SLOT` node, whatever the mark inside); the
//!   | AST has no node for the family. It names nodes by role, never by spelling, and has a node only
//!   | for what tomet interprets or what carries structure to walk: a hole is a [`Hole`], a type seat
//!   | a [`TypeSeat`], a name tag (`<PATH>`) is text. Merging nodes of different structure into one
//!   | type with a field to tell them apart is what split `Element` before; a new kind of seat is
//!   | judged by that rule.
//!   | A list item is a [`ListItem`]: it has no name, so it is not an [`Element`], whose name is
//!   | mandatory.
//!   | [`ListItem::as_element_view`] gives a zero-copy [`ElementView`] of the element a `-@name(...)`
//!   | item is.
//!   | What nodes share is [`Data`] (`(args)` and the `{...}` [`Group`]), and their body groups can
//!   | be inspected uniformly via [`BodyView`]; `[content]` and `#(id)` stay on the roles that take them.
//! - (Clear Crate Boundaries)
//!   | Semantics: Classification of element meanings (links, `::meta`, `::config`) lives in `tomet-semantics`.
//!   | Traversal & Manipulation: Generic tree traversal algorithms live in `tomet-tree`.
//!   | Formatting & Printing: Serialization to `.tmt` text lives in `tomet-printer`; how a key and a string are spelled, in `tomet-spelling`.
//!   | Validation: Field formatting and lint validation live in `tomet-validator`.
//! - (Span Equality Contract)
//!   | [`Span`] implements a custom `PartialEq` that always returns `true` so that structural
//!   | equality comparisons (`assert_eq!(doc1, doc2)`) ignore source positions. Use
//!   | [`Span::exact_eq()`] when exact source position comparisons are required.

#![allow(clippy::large_enum_variant)]

use serde::{Deserialize, Serialize};
use std::fmt::{self, Write as _};
use tomet_span::Span;

/// A pure data value: the subset of Tomet with a direct serde
/// equivalent (no headings, prose, or links).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Auto,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// A number kept as it was written, because reading it as an `i64` or an
    /// `f64` would change it: `1.10` (an `f64` prints `1.1`), `1e3`, `-0`, or an
    /// integer too large for an `i64`.
    ///
    /// Only these. A number that survives being read keeps being an
    /// [`Int`](Value::Int) or a [`Float`](Value::Float), so a value has one
    /// representation; whether `1.5` is a number is not in doubt, and whether
    /// `1.10` is a number, a version or a string is for the type that reads it
    /// to say. `1.10.0`, `007` and `2026-10-25` are not numbers by the JSON
    /// grammar and never come here: they are [`String`](Value::String)s.
    ///
    /// The text is a valid JSON number, so it parses as an `f64` (or fails only
    /// for a magnitude beyond one). Read it with
    /// [`as_number`](Value::as_number); the text it was written as, whatever the
    /// scalar, is [`scalar_text`](Value::scalar_text).
    Number(String),
    String(String),
    /// `(a, b, k: v)` -- an ordered run of entries, each with a
    /// name or without one. `()` is the empty tuple and `(x)` is a tuple with
    /// one unnamed entry. See [`Tuple`].
    Tuple(Tuple),
    /// `name(a, b, ...)` -- a call the parser did not resolve: a head and its
    /// fields.
    ///
    /// The parser resolves the literal constructors (`string`, `int`, `float`)
    /// to the plain value they stand for. Every other callee -- `list` (a
    /// type: `list(string)`), `enum`, a typo -- is kept here as an
    /// uninterpreted `String`, the same way the parser never judges a
    /// `::name(...)` connect's name; only `tomet-semantics` knows which of
    /// those are real. `list(a, b)` is not a tuple literal: write `(a, b)`.
    ///
    /// Deliberately not the same mechanism as `$func(args)`'s
    /// `InterpExprKind::Call`: that callee can itself be a path
    /// expression and is evaluated later, while this is a literal fixed
    /// at parse time. Sharing surface syntax (`name(args)`) is coincidence,
    /// not kinship -- conflating the two would blur which one is
    /// evaluated for a reader of either.
    Call(String, Tuple),
    /// `{$ ... }` / `$name(...)` sitting where a value goes -- evaluated later,
    /// so `tomet-semantics` never looks inside it as data. See [`Interpolation`].
    Interp(Interpolation),
    /// A `<?matcher(...)>` hole sitting where a value goes. See [`Hole`].
    Hole(Box<Hole>),
    /// A `<$type(...)>` seat sitting where a value goes. See [`TypeSeat`].
    TypeSeat(TypeSeat),
    /// A real, `@`-sigiled element sitting where a value goes --
    /// `::meta(icon: @doc.icon("triangle"))`. Unlike [`Value::Call`], this
    /// is the same [`Element`] any other position produces: it has a
    /// `name`, classifies and validates through `Bindings` like any other
    /// element (an unregistered name is still an error here), and can
    /// itself carry `args`/`content`/`value`. What distinguishes it from a
    /// call is exactly the `@` -- a call is inert, uninterpreted data
    /// (`tomet-semantics` never looks inside a `Value::Call`); an embedded
    /// element is a first-class element that happens to sit in a value
    /// slot, which is why [`crate::Entry::Element`] already lets one stand
    /// as a bare, keyless entry in a `{...}` group -- this variant is what
    /// lets the same thing be a named `key: @name(...)` value, or an
    /// unnamed entry in a tuple, too, since `Value` is the type those
    /// positions share.
    ///
    /// Parsed with `allow_colon_connect: false`, same as
    /// [`crate::Entry::Element`] -- `::name(...)` stays unavailable inside
    /// any value position, deliberately: a `{...}`/`(...)` group is data,
    /// and a connect changes what the *enclosing* element means, which
    /// has no sense for an element that is itself sitting inside a value.
    Element(Box<Element>),
    /// `key: [ ... ]` -- block content (the same `Vec<Block>` grammar as
    /// an element's own `[content]`/`|content` slot, see
    /// `tmtroot/docs/spec/feature/content-shape.tmt`) sitting as a named
    /// value inside `(args)`/`{data}`, e.g. `@conflict(a: [ ... ], b: [
    /// ... ])`. `[...]` means block content uniformly at any
    /// value position, as it does at the element level; a tuple is
    /// `(a, b)`, and `[a, b]` is not a list literal.
    Blocks(Vec<Block>),
}

/// One entry of a [`Tuple`]: a value, with a name or without one.
///
/// An entry with no name is an entry whose key was not written; its key is its position among
/// the unnamed entries of its tuple. An entry does not know that position: ask the tuple
/// ([`Tuple::keyed`], [`Tuple::get`]).
#[derive(Debug, Clone, PartialEq)]
pub struct TupleEntry<V = Value> {
    name: Option<String>,
    value: V,
}

impl<V> TupleEntry<V> {
    pub fn unnamed(value: V) -> Self {
        Self { name: None, value }
    }

    pub fn named(name: impl Into<String>, value: V) -> Self {
        Self {
            name: Some(name.into()),
            value,
        }
    }

    /// The name this entry is written with, or `None` for an unnamed entry.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn value(&self) -> &V {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut V {
        &mut self.value
    }

    pub fn into_value(self) -> V {
        self.value
    }

    pub fn into_parts(self) -> (Option<String>, V) {
        (self.name, self.value)
    }

    pub fn map_value<U>(self, f: impl FnOnce(V) -> U) -> TupleEntry<U> {
        TupleEntry {
            name: self.name,
            value: f(self.value),
        }
    }
}

/// A key written twice in one tuple: the key, and the index (among the entries given) of the
/// entry that repeats it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateKey {
    pub key: String,
    pub index: usize,
}

impl fmt::Display for DuplicateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "duplicate key `{}`", self.key)
    }
}

impl std::error::Error for DuplicateKey {}

/// An ordered run of [`TupleEntry`] items, each with a name or without one.
///
/// The one data container in the AST: the fields handed to a head (a nexus's `(args)`, the
/// `(...)` of a [`Value::Call`]) or, with no head written, a value of its own. There is no list
/// and no map as kinds of value: a tuple whose entries are all unnamed is what a list was, one
/// whose entries are all named what a map was, and a tuple may mix both.
///
/// =[ Keys ]
///
/// A key is text, and there is one key space: a key is not a value and has no type. Quoting a key
/// is spelling (`"a": 1` is `a: 1`), and so is leaving it out: an unnamed entry's key is its
/// position among the unnamed entries, in decimal (`0`, `1`, `12`). So `(a, b)` and
/// `("0": a, "1": b)` are the same tuple, and `("0": a, b)` writes the key `0` twice.
///
/// A tuple keeps that true by how it is built, not by asking its callers to:
///
/// - an entry stores a name only when it needs one. A name that is the position the entry would
///   have unnamed is dropped as the entry comes in, so the two spellings above are one value,
///   equal by `==`. No entry stores a position: removing or naming an unnamed entry moves the
///   ones after it by itself.
/// - no key is held twice. Every way to build or change a tuple that could repeat one is
///   fallible and returns [`DuplicateKey`]; nothing else can add an entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Tuple<V = Value> {
    entries: Vec<TupleEntry<V>>,
}

impl<V> Default for Tuple<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

/// The decimal a position is written as, if `name` is one: no sign, no leading zero.
fn as_position(name: &str) -> Option<usize> {
    let canonical = name == "0" || (!name.starts_with('0') && !name.is_empty());
    if canonical && name.bytes().all(|b| b.is_ascii_digit()) {
        name.parse().ok()
    } else {
        None
    }
}

impl<V> Tuple<V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The tuple of `entries`, in the order given, or the first key that is written twice.
    ///
    /// This is the one place the rules of a tuple's keys are applied; every other constructor
    /// and every change goes through it.
    pub fn try_from_entries(
        entries: impl IntoIterator<Item = TupleEntry<V>>,
    ) -> Result<Self, DuplicateKey> {
        let mut tuple = Self::new();
        for (index, entry) in entries.into_iter().enumerate() {
            tuple
                .try_push(entry)
                .map_err(|duplicate| DuplicateKey { index, ..duplicate })?;
        }
        Ok(tuple)
    }

    /// Adds `entry` at the end, unless its key is already held.
    pub fn try_push(&mut self, entry: TupleEntry<V>) -> Result<(), DuplicateKey> {
        let next = self.unnamed_len();
        let duplicate = |key: String| DuplicateKey {
            key,
            index: self.entries.len(),
        };
        let entry = match entry.name.as_deref().map(|name| (name, as_position(name))) {
            // Named with the position it would have unnamed: the same entry, unnamed.
            Some((_, Some(position))) if position == next => TupleEntry::unnamed(entry.value),
            Some((name, Some(position))) if position < next => {
                return Err(duplicate(name.to_string()));
            }
            Some((name, _)) if self.entries.iter().any(|e| e.name() == Some(name)) => {
                return Err(duplicate(name.to_string()));
            }
            Some(_) => entry,
            None => entry,
        };
        if entry.name.is_none() {
            let key = next.to_string();
            if self.entries.iter().any(|e| e.name() == Some(key.as_str())) {
                return Err(duplicate(key));
            }
        }
        self.entries.push(entry);
        Ok(())
    }

    /// The tuple of `entries`, for entries known not to repeat a key (a literal in code).
    ///
    /// # Panics
    ///
    /// If a key is given twice. Use [`Tuple::try_from_entries`] for entries that are data.
    pub fn from_entries(entries: impl IntoIterator<Item = TupleEntry<V>>) -> Self {
        Self::try_from_entries(entries)
            .unwrap_or_else(|duplicate| panic!("Tuple::from_entries: {duplicate}"))
    }

    /// A tuple of unnamed entries. It cannot repeat a key.
    pub fn from_unnamed(values: impl IntoIterator<Item = V>) -> Self {
        Self {
            entries: values.into_iter().map(TupleEntry::unnamed).collect(),
        }
    }

    /// A tuple of named entries, for names known to differ (a literal, the keys of a map).
    ///
    /// # Panics
    ///
    /// If a key is given twice. Use [`Tuple::try_from_entries`] for names that are data.
    pub fn from_named(pairs: impl IntoIterator<Item = (String, V)>) -> Self {
        Self::from_entries(
            pairs
                .into_iter()
                .map(|(name, value)| TupleEntry::named(name, value)),
        )
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn unnamed_len(&self) -> usize {
        self.entries.iter().filter(|e| e.name.is_none()).count()
    }

    /// Whether no entry has a name. True for an empty tuple.
    pub fn has_no_named(&self) -> bool {
        self.entries.iter().all(|e| e.name.is_none())
    }

    /// Whether every entry has a name. True for an empty tuple.
    pub fn has_no_unnamed(&self) -> bool {
        self.entries.iter().all(|e| e.name.is_some())
    }

    /// The index, among the entries as written, of the entry whose key is `key`.
    pub fn index_of(&self, key: &str) -> Option<usize> {
        let position = as_position(key);
        let mut seen = 0;
        self.entries.iter().position(|entry| match &entry.name {
            Some(name) => name == key,
            None => {
                seen += 1;
                position == Some(seen - 1)
            }
        })
    }

    /// The value of the entry whose key is `key`: a name, or a position in decimal.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.index_of(key).map(|index| &self.entries[index].value)
    }

    /// Mutable counterpart of [`Tuple::get`].
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.index_of(key)
            .map(|index| &mut self.entries[index].value)
    }

    /// Whether an entry has the key `key`.
    pub fn contains_key(&self, key: &str) -> bool {
        self.index_of(key).is_some()
    }

    /// The `n`-th entry as written.
    pub fn nth(&self, n: usize) -> Option<&TupleEntry<V>> {
        self.entries.get(n)
    }

    /// Mutable counterpart of [`Tuple::nth`]: the entry's value can change, its name cannot.
    pub fn nth_mut(&mut self, n: usize) -> Option<&mut TupleEntry<V>> {
        self.entries.get_mut(n)
    }

    /// The `n`-th unnamed entry's value: the entry whose key is `n`.
    pub fn nth_unnamed(&self, n: usize) -> Option<&V> {
        self.unnamed().nth(n)
    }

    /// Mutable counterpart of [`Tuple::nth_unnamed`].
    pub fn nth_unnamed_mut(&mut self, n: usize) -> Option<&mut V> {
        self.entries
            .iter_mut()
            .filter(|e| e.name.is_none())
            .nth(n)
            .map(|e| &mut e.value)
    }

    /// All entries in written order.
    pub fn entries(&self) -> std::slice::Iter<'_, TupleEntry<V>> {
        self.entries.iter()
    }

    /// Owning iteration over all entries in written order.
    pub fn into_entries(self) -> std::vec::IntoIter<TupleEntry<V>> {
        self.entries.into_iter()
    }

    /// Every entry with its key, in written order: the name of a named entry, the position of an
    /// unnamed one.
    pub fn keyed(&self) -> impl Iterator<Item = (std::borrow::Cow<'_, str>, &V)> {
        let mut position = 0;
        self.entries.iter().map(move |entry| {
            let key = match &entry.name {
                Some(name) => std::borrow::Cow::Borrowed(name.as_str()),
                None => {
                    position += 1;
                    std::borrow::Cow::Owned((position - 1).to_string())
                }
            };
            (key, &entry.value)
        })
    }

    /// The values of all entries in written order.
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|e| &e.value)
    }

    /// Mutable counterpart of [`Tuple::values`]. A value can change in place; a name cannot.
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.entries.iter_mut().map(|e| &mut e.value)
    }

    /// This tuple with every value passed through `f`. The keys are untouched.
    pub fn map_values<U>(self, mut f: impl FnMut(V) -> U) -> Tuple<U> {
        Tuple {
            entries: self
                .entries
                .into_iter()
                .map(|e| TupleEntry {
                    name: e.name,
                    value: f(e.value),
                })
                .collect(),
        }
    }

    /// This tuple with every value passed through fallible `f`. The keys are untouched.
    pub fn try_map_values<U, E>(self, mut f: impl FnMut(V) -> Result<U, E>) -> Result<Tuple<U>, E> {
        let mut entries = Vec::with_capacity(self.entries.len());
        for e in self.entries {
            entries.push(TupleEntry {
                name: e.name,
                value: f(e.value)?,
            });
        }
        Ok(Tuple { entries })
    }

    /// The named entries as `(name, value)`, in written order.
    pub fn named(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries
            .iter()
            .filter_map(|e| Some((e.name.as_deref()?, &e.value)))
    }

    /// Mutable counterpart of [`Tuple::named`].
    pub fn named_mut(&mut self) -> impl Iterator<Item = (&str, &mut V)> {
        self.entries
            .iter_mut()
            .filter_map(|e| Some((e.name.as_deref()?, &mut e.value)))
    }

    /// Owning counterpart of [`Tuple::named`].
    pub fn into_named(self) -> impl Iterator<Item = (String, V)> {
        self.entries
            .into_iter()
            .filter_map(|e| Some((e.name?, e.value)))
    }

    /// The unnamed entries' values, in written order: the `n`-th has the key `n`.
    pub fn unnamed(&self) -> impl Iterator<Item = &V> {
        self.entries
            .iter()
            .filter_map(|e| e.name.is_none().then_some(&e.value))
    }

    /// Owning counterpart of [`Tuple::unnamed`].
    pub fn into_unnamed(self) -> impl Iterator<Item = V> {
        self.entries
            .into_iter()
            .filter_map(|e| e.name.is_none().then_some(e.value))
    }

    /// Adds an unnamed entry at the end, unless an entry is named with its position.
    pub fn try_push_unnamed(&mut self, value: V) -> Result<(), DuplicateKey> {
        self.try_push(TupleEntry::unnamed(value))
    }

    /// Adds a named entry at the end, unless its key is already held.
    pub fn try_push_named(
        &mut self,
        name: impl Into<String>,
        value: V,
    ) -> Result<(), DuplicateKey> {
        self.try_push(TupleEntry::named(name, value))
    }

    /// Gives the key `key` the value `value`: in place if an entry has that key, as a new entry
    /// at the end if none has. It cannot repeat a key.
    pub fn set(&mut self, key: &str, value: V) {
        match self.index_of(key) {
            Some(index) => self.entries[index].value = value,
            None => self
                .try_push_named(key, value)
                .expect("no entry has this key, so adding it repeats none"),
        }
    }

    /// Names the entry whose key is `key` with `name`, where it stands. `Ok(false)` if no entry
    /// has the key `key`; an error, and no change, if another entry already has the key `name`.
    /// Naming an unnamed entry moves the unnamed ones after it down by one.
    pub fn rename(&mut self, key: &str, name: impl Into<String>) -> Result<bool, DuplicateKey>
    where
        V: Clone,
    {
        let Some(index) = self.index_of(key) else {
            return Ok(false);
        };
        let mut entries = self.entries.clone();
        entries[index].name = Some(name.into());
        *self = Self::try_from_entries(entries)?;
        Ok(true)
    }

    /// Removes the entry whose key is `key` and returns its value. The unnamed entries after a
    /// removed unnamed one move down by one, as their keys are their positions.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        let index = self.index_of(key)?;
        Some(self.entries.remove(index).value)
    }
}

impl<V> IntoIterator for Tuple<V> {
    type Item = TupleEntry<V>;
    type IntoIter = std::vec::IntoIter<TupleEntry<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a Tuple<V> {
    type Item = &'a TupleEntry<V>;
    type IntoIter = std::slice::Iter<'a, TupleEntry<V>>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

/// By its keys: no entry is `{}`; keys that are `0`, `1`, ... in the order written (every entry
/// unnamed) are an array; anything else is an object keyed by the keys.
impl Serialize for Tuple {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if !self.is_empty() && self.has_no_named() {
            use serde::ser::SerializeSeq;
            let mut s = serializer.serialize_seq(Some(self.len()))?;
            for value in self.values() {
                s.serialize_element(value)?;
            }
            s.end()
        } else {
            use serde::ser::SerializeMap;
            let mut m = serializer.serialize_map(Some(self.len()))?;
            for (key, value) in self.keyed() {
                m.serialize_entry(key.as_ref(), value)?;
            }
            m.end()
        }
    }
}

/// An array gives the keys `0`, `1`, ...; an object its keys. `{"0": "a"}` and `["a"]` are the
/// same tuple. An object that repeats a key is an error.
impl<'de> Deserialize<'de> for Tuple {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TupleVisitor;

        impl<'de> serde::de::Visitor<'de> for TupleVisitor {
            type Value = Tuple;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an array or an object")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(item) = seq.next_element()? {
                    values.push(item);
                }
                Ok(Tuple::from_unnamed(values))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut tuple = Tuple::new();
                while let Some((k, v)) = map.next_entry::<String, Value>()? {
                    tuple
                        .try_push_named(k, v)
                        .map_err(serde::de::Error::custom)?;
                }
                Ok(tuple)
            }
        }

        deserializer.deserialize_any(TupleVisitor)
    }
}

impl Value {
    /// The text a bare scalar was written as, for a target that wants text: a
    /// `string` argument given `2024`, `true` or `1.10`.
    ///
    /// `None` for what is not a scalar (`none`, a tuple, a call, an element) --
    /// those have no single text. A [`String`](Value::String) is itself,
    /// whether it was quoted or not: the reader does not keep the difference.
    pub fn scalar_text(&self) -> Option<std::borrow::Cow<'_, str>> {
        use std::borrow::Cow;
        Some(match self {
            Value::Auto => Cow::Borrowed("auto"),
            Value::String(s) | Value::Number(s) => Cow::Borrowed(s.as_str()),
            Value::Int(i) => Cow::Owned(i.to_string()),
            Value::Float(f) => Cow::Owned(tomet_scalar::float_text(*f)),
            Value::Bool(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
            _ => return None,
        })
    }

    /// The number a [`Float`](Value::Float) or [`Number`](Value::Number) is, as
    /// an `f64`. A `Number` too large for one is `None`. (An [`Int`](Value::Int)
    /// is not read as a float here: the caller asked for the kept-as-written
    /// kinds, and has `as_i64` for the rest.)
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Number(text) => text.parse::<f64>().ok().filter(|f| f.is_finite()),
            _ => None,
        }
    }
}

impl From<tomet_scalar::Scalar> for Value {
    fn from(s: tomet_scalar::Scalar) -> Self {
        match s {
            tomet_scalar::Scalar::None => Value::None,
            tomet_scalar::Scalar::Auto => Value::Auto,
            tomet_scalar::Scalar::Bool(b) => Value::Bool(b),
            tomet_scalar::Scalar::Int(i) => Value::Int(i),
            tomet_scalar::Scalar::Float(f) => Value::Float(f),
            tomet_scalar::Scalar::Number(n) => Value::Number(n),
            tomet_scalar::Scalar::Text(t) => Value::String(t),
        }
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::None => serializer.serialize_none(),
            Value::Auto => Err(serde::ser::Error::custom("`auto` has no JSON representation")),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Int(i) => serializer.serialize_i64(*i),
            Value::Float(f) => serializer.serialize_f64(*f),
            // A number on the wire is an f64: that is the most a serializer
            // that has no arbitrary-precision number can say. One that cannot
            // be one (too large) is sent as the text it was written as.
            Value::Number(text) => match text.parse::<f64>() {
                Ok(f) if f.is_finite() => serializer.serialize_f64(f),
                _ => serializer.serialize_str(text),
            },
            Value::String(s) => serializer.serialize_str(s),
            Value::Tuple(tuple) => tuple.serialize(serializer),
            Value::Call(name, args) => {
                use serde::ser::SerializeMap;
                let mut m = serializer.serialize_map(Some(2))?;
                m.serialize_entry("call", name)?;
                m.serialize_entry("args", args)?;
                m.end()
            }
            Value::Interp(interp) => interp.serialize(serializer),
            Value::Hole(hole) => hole.serialize(serializer),
            Value::TypeSeat(seat) => seat.serialize(serializer),
            Value::Element(el) => el.serialize(serializer),
            Value::Blocks(blocks) => {
                use serde::ser::SerializeSeq;
                let mut s = serializer.serialize_seq(Some(blocks.len()))?;
                for block in blocks {
                    s.serialize_element(block)?;
                }
                s.end()
            }
        }
    }
}

// Deliberately no `Value::Call`, `Value::Interp`, `Value::Hole`, `Value::TypeSeat`,
// `Value::Element`, or `Value::Blocks` arm below: all six are only ever produced by
// `tomet-syntax-parser` reading source text (`name(...)` / `{$ ... }` /
// `<$type>` / `@name(...)` / `key: [...]`), never by
// deserializing inbound data -- there is no `visit_call` and nothing in
// serde's data model looks like an `Element` or a `Vec<Block>` either. This
// is not an oversight.
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ValueVisitor;

        impl<'de> serde::de::Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("any valid Tomet data value")
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
                Ok(Value::Bool(v))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
                Ok(Value::Int(v))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
                Ok(Value::Int(v as i64))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> {
                Ok(Value::Float(v))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Value::String(v.to_string()))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
                Ok(Value::String(v))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(Value::None)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(Value::None)
            }

            // An array and an object are a tuple, read as `Tuple` reads them.
            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                Tuple::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))
                    .map(Value::Tuple)
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                Tuple::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(Value::Tuple)
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Paragraph(Paragraph),
    Element(Element),
    Section(Section),
    /// A `-`/`-.` list, first-class like [`Section`] rather than encoded as
    /// a named [`Element`]. See [`List`].
    List(List),
    /// A `<?matcher(...)>` hole standing as a block. See [`Hole`].
    Hole(Hole),
    /// A `<$type(...)>` seat standing as a block. See [`TypeSeat`].
    TypeSeat(TypeSeat),
    /// A `{$ ... }` that stands alone on its line, in block context. It is a
    /// block of its own, and not a one-item paragraph, because what it
    /// evaluates to may be blocks (an include, a generated table).
    Interp(Interpolation),
}

impl Block {
    pub fn span(&self) -> Span {
        match self {
            Block::Paragraph(p) => p.span,
            Block::Element(e) => e.span,
            Block::Section(s) => s.span,
            Block::List(l) => l.span,
            Block::Hole(h) => h.span,
            Block::TypeSeat(t) => t.span,
            Block::Interp(i) => i.span,
        }
    }
}

/// A hierarchical section: introduced by `=`, `==`, etc.
///
/// Contains its nesting level (`=` is 1, `==` is 2, etc.), title inlines,
/// optional arguments (`(id: intro)`), optional value group (`{attrs}`),
/// connects (`::as(...)`), and child blocks (paragraphs, lists, sub-sections)
/// scoped to this section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub level: usize,
    pub title: Vec<Inline>,
    pub data: Data,
    pub id: Option<Id>,
    pub connects: Vec<Connect>,
    pub blocks: Vec<Block>,
    pub span: Span,
}

impl Section {
    pub fn new(level: usize, title: Vec<Inline>, span: Span) -> Self {
        Self {
            level,
            title,
            data: Data::default(),
            id: None,
            connects: Vec::new(),
            blocks: Vec::new(),
            span,
        }
    }
}

/// A `-` (unordered) or `-.` (ordered) list: first-class, like [`Section`],
/// rather than a named [`Element`] holding bare sub-`Element`s.
///
/// No `args`/`content`/`value`, unlike `Section` -- none of the three had a
/// concrete use case (a caption for the whole list, attrs on the list as a
/// whole, an ordered list's starting number) when this type was designed;
/// add one later if a real want surfaces, with the use case stated
/// alongside the field. `id`/`connects` were kept: `id` mirrors `Section`'s
/// own anchor-reference use (`#(id)` on the whole list), and `connects` is
/// what a `::rule(allow:(...))` attached to the list itself (restricting
/// what its own items may be) needs to live on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct List {
    pub ordered: bool,
    pub items: Vec<ListItem>,
    pub id: Option<Id>,
    pub connects: Vec<Connect>,
    pub span: Span,
}

impl List {
    pub fn new(ordered: bool, items: Vec<ListItem>, span: Span) -> Self {
        Self {
            ordered,
            items,
            id: None,
            connects: Vec::new(),
            span,
        }
    }
}

/// One item of a [`List`]: a plain `- content`, or `-@name(...) content`
/// (the combine notation), which is the item *and* an element of that name.
///
/// It takes the groups an element does -- `(args)` is the item's marker,
/// `{value}` its attributes, `#(id)` and `::connect`s -- and its `content` is
/// always there (possibly empty). It has no placement: an item is always a
/// block of its own list. `name` is `None` for a plain item and the element's
/// name for the combine notation.
///
/// `sublist` is the one genuinely new thing: an indented list directly
/// below this item. Typed as `Option<List>`, not `Option<Vec<Block>>`,
/// because the parser only ever produces one specific shape here (another
/// list) -- giving it its own field makes that a compiler-checked fact
/// instead of a convention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub name: Option<Name>,
    pub data: Data,
    pub content: Vec<Block>,
    pub id: Option<Id>,
    pub connects: Vec<Connect>,
    pub sublist: Option<List>,
    pub span: Span,
}

impl ListItem {
    /// A zero-copy borrowed view of the element an item written in combine notation
    /// (`-@name(...) content`) is. Returns `None` for a plain list item without a name.
    pub fn as_element_view(&self) -> Option<ElementView<'_>> {
        let name = self.name.as_ref()?;
        Some(ElementView {
            name,
            placement: Placement::Block,
            data: &self.data,
            content: Some(&self.content),
            id: self.id.as_ref(),
            connects: &self.connects,
            span: self.span,
        })
    }

    /// A zero-copy borrowed view of this item's body groups.
    pub fn as_body_view(&self) -> BodyView<'_> {
        BodyView {
            data: &self.data,
            content: Some(&self.content),
            id: self.id.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    pub content: Vec<Inline>,
    pub span: Span,
}

impl Paragraph {
    pub fn new(content: Vec<Inline>, span: Span) -> Self {
        Self { content, span }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Inline {
    Text(Text),
    Element(Element),
    /// `{$ ... }` / `$name(...)` inside running text. See [`Interpolation`].
    Interp(Interpolation),
    /// A `<?matcher(...)>` hole inside running text. See [`Hole`].
    Hole(Hole),
    /// A `<$type(...)>` seat inside running text. See [`TypeSeat`].
    TypeSeat(TypeSeat),
    /// A source line break that is semantically whitespace. Unlike
    /// folding it into a literal `' '` (or nothing, between two
    /// East-Asian-wide characters) at parse time, this keeps the break's
    /// existence in the tree instead of destroying it -- there is no way to
    /// recover "the author wrapped a line" from a plain space once it has
    /// been baked into a `Text.value`. How it is realized (a space, a literal
    /// newline, nothing) is left to whichever consumer renders the tree, the
    /// same way CommonMark's own softbreak leaves that choice to the
    /// renderer.
    SoftBreak(SoftBreak),
    /// A real forced line break: always rendered as one, never folded.
    LineBreak(LineBreak),
    /// Verbatim text that may legitimately contain `'\n'`/`'\r'` (currently
    /// only fenced code block bodies). Kept as its own variant so `Text`
    /// carries a real invariant: it never contains a raw newline.
    Raw(RawText),
}

impl Inline {
    /// The first character of this node's own text, if it has one -- `Text`
    /// and `Raw` do, `Element`/`Interp`/`Hole`/`TypeSeat`/`SoftBreak`/`LineBreak` don't.
    ///
    /// Used by renderers reproducing [`softbreak_join`]'s wide-character
    /// check against whatever inline follows a `SoftBreak`: only the
    /// immediate next item is consulted, deliberately not skipping past an
    /// `Element` to find text beyond it, the same as the fold, which only
    /// ever sees characters within its own text run.
    pub fn first_char(&self) -> Option<char> {
        match self {
            Inline::Text(t) => t.value.chars().next(),
            Inline::Raw(t) => t.value.chars().next(),
            Inline::Element(_)
            | Inline::Interp(_)
            | Inline::Hole(_)
            | Inline::TypeSeat(_)
            | Inline::SoftBreak(_)
            | Inline::LineBreak(_) => None,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Inline::Text(t) => t.span,
            Inline::Element(e) => e.span,
            Inline::Interp(i) => i.span,
            Inline::Hole(h) => h.span,
            Inline::TypeSeat(t) => t.span,
            Inline::SoftBreak(b) => b.span,
            Inline::LineBreak(b) => b.span,
            Inline::Raw(t) => t.span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SoftBreak {
    pub span: Span,
}

/// What a [`SoftBreak`] renders as when it is folded into plain running
/// text, given the character immediately before and after it.
///
/// This is the fold rule tomet has always applied when joining a wrapped
/// line: a space, except between two East-Asian-wide characters, where a
/// space would open a visible gap in the middle of a sentence. It lives
/// here, shared, because several independent call sites need it: HTML, Typst, and
/// Pandoc export always fold this way, and so do the plain-text-only
/// corners of the printer and Markdown export (an image's `alt`, a
/// codeblock's extracted text) that cannot themselves contain a literal
/// newline. The printer's and Markdown export's own *prose* rendering
/// chooses differently -- a real `'\n'`, to keep a wrapped line looking
/// wrapped -- which is exactly the point of leaving this choice to the
/// renderer instead of deciding it once in the parser.
pub fn softbreak_join(before: Option<char>, after: Option<char>) -> &'static str {
    fn is_wide(c: char) -> bool {
        unicode_width::UnicodeWidthChar::width(c) == Some(2)
    }
    if before.is_some_and(is_wide) && after.is_some_and(is_wide) {
        ""
    } else {
        " "
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineBreak {
    pub span: Span,
}

/// Verbatim text, captured byte-for-byte from source. See
/// [`Inline::Raw`] for why this is a separate type from [`Text`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawText {
    pub value: String,
    pub span: Span,
}

impl RawText {
    pub fn new(value: impl Into<String>, span: Span) -> Self {
        Self {
            value: value.into(),
            span,
        }
    }
}

impl From<String> for RawText {
    fn from(value: String) -> Self {
        Self {
            value,
            span: Span::dummy(),
        }
    }
}

impl From<&str> for RawText {
    fn from(value: &str) -> Self {
        Self {
            value: value.to_string(),
            span: Span::dummy(),
        }
    }
}

/// A run of prose text, folded from source the way any wrapped inline
/// content is. Invariant: `value` never contains `'\n'` or `'\r'` -- a source
/// line break becomes an [`Inline::SoftBreak`] or [`Inline::LineBreak`]
/// instead of a character here. Verbatim text that may contain real newlines
/// (code block bodies) is [`RawText`], not this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub value: String,
    pub span: Span,
}

impl Text {
    pub fn new(value: impl Into<String>, span: Span) -> Self {
        Self {
            value: value.into(),
            span,
        }
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        Self {
            value,
            span: Span::dummy(),
        }
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Self {
            value: value.to_string(),
            span: Span::dummy(),
        }
    }
}

impl std::ops::Deref for Text {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl std::ops::DerefMut for Text {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// An element's name, split into its optional namespace and its own name.
///
/// The separator is `.`: `deck.bookmark` is `Name { namespace:
/// Some("deck"), name: "bookmark" }`. Exactly two namespaces may be
/// written bare: `std`, which the language carries, and the document's
/// own `::kind`, declared on its first line. A name from any other
/// namespace -- anything brought in with `::use` -- is always written
/// out, and a bare name in neither of the two is an error rather than
/// falling back to a `Custom` kind. That check is `Bindings::classify`
/// in `tomet-semantics`, not the free `classify_std` beside it, which only
/// knows `std`. This type only records the split.
///
/// Both halves are ASCII identifiers (`[A-Za-z_][A-Za-z0-9_-]*`). `.` is
/// the separator, so it is deliberately *not* an identifier character
/// here -- unlike in entry names, where `url.wiki` is one flat name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Name {
    pub namespace: Option<String>,
    pub name: String,
}

impl Name {
    /// A bare, un-namespaced name.
    pub fn bare(name: impl Into<String>) -> Self {
        Name {
            namespace: None,
            name: name.into(),
        }
    }

    /// A namespaced name.
    pub fn namespaced(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Name {
            namespace: Some(namespace.into()),
            name: name.into(),
        }
    }

    /// Whether this is a bare name, i.e. one reserved for Tomet's own
    /// vocabulary.
    pub fn is_bare(&self) -> bool {
        self.namespace.is_none()
    }

    /// Whether this is the bare name `name`: `rule` is, `ns.rule` is not.
    pub fn is_bare_named(&self, name: &str) -> bool {
        self.is_bare() && self.name == name
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{ns}.{}", self.name),
            None => f.write_str(&self.name),
        }
    }
}

/// An element's own id, written `#(foobar)` right after its own
/// `(args)[content]{value}` groups and before any `::name(...)` connect.
///
/// A plain `String`, not [`Value`]: `#(...)` is parsed with a narrower,
/// scalar-only grammar (a bare identifier-like token or a quoted
/// string) than the general value grammar `(args)`/`{value}` use, so a
/// tuple or call can never end up here -- unlike `Name`, there is no
/// namespace concept for an id. No ASCII restriction, unlike [`Name`]:
/// the content is a value, not a structural identifier, so it is as
/// free as any other string value (e.g. `@line(天音かなた)` elsewhere in
/// this codebase already allows non-ASCII value text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Id(pub String);

/// Where an element sits: on its own as a block, or inside running text.
///
/// The parser derives this from position alone -- never from the element's
/// name, so the vocabulary-free invariant holds. An element is
/// [`Placement::Block`] when it is in block context (document start, after
/// a blank line, or after a block closed) *and* it ends its line; anything
/// else is [`Placement::Inline`].
///
/// It is recorded rather than implied by the tree because [`Element`]'s
/// `content` is a `Vec<Inline>`: an element written at a line start inside
/// another element's `[content]` is a block, and this field is the only
/// place that survives.
///
/// This is placement, not shape. Which shape an element is *allowed* to
/// take is a vocabulary question, answered by `tomet-semantics`'
/// `required_shape` and checked against this field by `shape_mismatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Placement {
    Block,
    #[default]
    Inline,
}

/// A `::name(args){data}[content]#(id)` declaration about the file itself,
/// not content in it.
///
/// It takes the groups an element does and is introduced by `::` instead of
/// `@`. The sigil says *where* it belongs -- in the file's preamble header
/// ([`File::header`]), the run of `::` declarations at the top -- so the preamble can be
/// found without loading any vocabulary. Which names may be written this way is the
/// `Region` a vocabulary declares for them, and whether one takes `[content]`
/// is that vocabulary's call too, so the parser does not decide it.
///
/// It sits in [`File::header`]: a `::` has to open its line and stand alone on it,
/// and so has no [`Placement`]. And it is attached to nothing, so unlike an
/// [`Element`] it takes no `::connect`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Declaration {
    pub name: Name,
    pub data: Data,
    pub content: Option<Vec<Block>>,
    pub id: Option<Id>,
    pub span: Span,
}

impl Declaration {
    /// A zero-copy borrowed view of this declaration's body groups.
    pub fn as_body_view(&self) -> BodyView<'_> {
        BodyView {
            data: &self.data,
            content: self.content.as_deref(),
            id: self.id.as_ref(),
        }
    }
}

/// A `<?matcher(name, args){options}>[content]` hole: the description of a value that is
/// not there yet, in a pattern.
///
/// It is a box, `<...>`, whose head (the `?` mark, the matcher and the `(args)` and `{options}`
/// groups) is inside and whose `[content]` follows the `>`. Its groups are read by the
/// same code as an element's. Differences: its head is a *matcher*, not an element name, and it is
/// optional (`<?>` and `<?(n)>` have none); and it takes no `::connect`. A box that does not close
/// or does not fit is text, so ordinary prose such as `本当に?(はい)` or `a < b` is never a hole.
///
/// A hole sits where an element sits, so it is a variant of [`Block`], [`Inline`], [`Entry`]
/// and [`Value`], in the same tree a document is: a pattern is written in, and printed as,
/// ordinary tomet, and `parse_document` reads one like any other construct.
///
/// The matcher is not an element name: `<?meta>` is not the `meta` element. What a matcher
/// means, and what a hole matches, is the host's (tandr's), not tomet's. See
/// `tmtroot/spec/syntax/hole.tmt`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hole {
    pub matcher: Option<Name>,
    pub placement: Placement,
    pub data: Data,
    pub content: Option<Vec<Block>>,
    pub span: Span,
}

impl Hole {
    /// A zero-copy borrowed view of this hole's body groups.
    pub fn as_body_view(&self) -> BodyView<'_> {
        BodyView {
            data: &self.data,
            content: self.content.as_deref(),
            id: None,
        }
    }
}

/// A `<$type(args)>` seat: the type of a value that is not there yet, written where the value
/// goes. `<$date(default: auto)>`, `<$list(string, length: 6)>`, `<$uuid>`.
///
/// It is a box, `<...>`, whose head is the `$` mark and the type, as the expression it is: a
/// bare name (`uuid`) or a call of one (`list(string, length: 6)`), with its arguments in `()`
/// and nowhere else. `<$uuid>` and `<$uuid()>` are two written forms, a name and a call with no
/// arguments, and stay two here. It takes no `{value}`, no `[content]`, no `#(id)` and no
/// `::connect`, so it holds the type and a span and nothing else: it is not a [`Hole`], which
/// has a matcher and groups, and not an [`Interpolation`], which is evaluated where it stands.
///
/// `tomet-ast` never interprets the expression: which type names are valid, what their arguments
/// mean, and that `auto` in `default: auto` is not a variable, are for `tomet-type` and the
/// semantics. To the AST `auto` is a name like any other.
///
/// A seat sits where a value, a block or running text does, so it is a variant of [`Block`],
/// [`Inline`] and [`Value`], and not of [`Entry`]: a `{...}` group holds `key: value` entries
/// and elements, and a seat is neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeSeat {
    pub expr: InterpExpr,
    pub span: Span,
}

/// Displays the seat as source: `<$`, the type as [`InterpExpr`] writes it, and `>`. The text
/// reads back as the same seat.
impl fmt::Display for TypeSeat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<${}>", self.expr)
    }
}

/// A `::name(...)` connect stacked after an element, a section or a list
/// (`@x(...)::as(y)::rule(...)`): metadata about its owner, not content of the
/// document. It takes `(args)`, `[content]` and `{value}` like an element does,
/// and nothing else: no `#(id)` (an id belongs to the owner, written before the
/// first connect), no connects of its own, and no placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connect {
    pub name: Name,
    pub data: Data,
    pub content: Option<Vec<Block>>,
    pub span: Span,
}

impl Connect {
    /// A zero-copy borrowed view of this connect's body groups.
    pub fn as_body_view(&self) -> BodyView<'_> {
        BodyView {
            data: &self.data,
            content: self.content.as_deref(),
            id: None,
        }
    }
}

/// The data a node carries: its `(args)` tuple and its `{value}` group, each
/// optional and at most one of each.
///
/// Every node that takes either holds them as a `Data`, so what reads them
/// (the walkers, the printer, `merge_values`) is written once for all of
/// them. `[content]` and `#(id)` are not in here on purpose: they differ in
/// kind (child blocks, an address) and not every node takes them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Data {
    pub args: Option<Tuple>,
    pub value: Option<Group>,
}

/// An `@name(args)[content]{value}` element.
///
/// The one element sigil is `@`; it says "an element starts here" and nothing
/// else. Whether the element stands as a block or belongs to a paragraph is
/// [`Placement`], which the parser derives from position, and origin is
/// carried by [`Name`]'s namespace. Two other axes would carry no
/// information: `<T>` vs `@name` (official vs user-defined, but both
/// classified through the same arm) and `#name` vs `@name` (shape, which
/// `tomet-semantics`' `required_shape` already knew). `#(id)` is not a second
/// sigil either: it is [`Element::id`], a slot in the element's own groups.
///
/// A node that has no name of its own, a plain list item, is not an element at all.
///
/// `(args)` / `[content]` / `{value}`, each optional and at most one of each,
/// in any order in the source.
///
/// `id` is a single slot, written `#(foobar)`, read after those three
/// groups but strictly before any connect -- see [`Id`].
///
/// `connects` is unrelated to those three: it is the `::name(...)` family
/// stacked after them (`@x(...)::as(y)::rule(...)`), zero or more, each a
/// [`Connect`]. Order-preserving, empty when nothing follows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    /// The name after `@`. It is mandatory: a nameless `@(url:...)` is not an element,
    /// because a kind is not inferred from a key in the args.
    pub name: Name,
    pub placement: Placement,
    pub data: Data,
    /// `[content]` / `|content`, parsed with the exact same recursive block
    /// grammar as `file.body`/`Section.blocks`: a bare run with no
    /// block markers becomes `[Paragraph(...)]`, and blank lines, `=[...]`
    /// headings, lists etc. inside the brackets/pipe-run nest as their own
    /// `Block`s, same as at the top level. Whether a *given* element's
    /// vocabulary actually permits more than one block (or anything beyond
    /// inline-shaped content) here is not this field's concern -- the
    /// parser builds this uniformly, with no branch on the element's name,
    /// and `tomet-semantics` enforces legality downstream (builtin
    /// content-shape table for `BUILTIN_KINDS`, `@content{allow:}` for a
    /// vocabulary's own elements). See `tmtroot/docs/spec/feature/
    /// content-shape.tmt`.
    pub content: Option<Vec<Block>>,
    pub id: Option<Id>,
    pub connects: Vec<Connect>,
    pub span: Span,
}

impl Element {
    /// A zero-copy borrowed view of this element's fields.
    pub fn as_view(&self) -> ElementView<'_> {
        ElementView {
            name: &self.name,
            placement: self.placement,
            data: &self.data,
            content: self.content.as_deref(),
            id: self.id.as_ref(),
            connects: &self.connects,
            span: self.span,
        }
    }

    /// A zero-copy borrowed view of this element's body groups.
    pub fn as_body_view(&self) -> BodyView<'_> {
        BodyView {
            data: &self.data,
            content: self.content.as_deref(),
            id: self.id.as_ref(),
        }
    }
}

/// A zero-copy borrowed view of an element's fields.
///
/// Represents an [`Element`] or any node that stands as an element (such as a
/// combine list item `-@name(...)` via [`ListItem::as_element_view`]) without
/// heap allocation or cloning child blocks and data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementView<'a> {
    pub name: &'a Name,
    pub placement: Placement,
    pub data: &'a Data,
    pub content: Option<&'a [Block]>,
    pub id: Option<&'a Id>,
    pub connects: &'a [Connect],
    pub span: Span,
}

impl<'a> ElementView<'a> {
    /// Positional or keyword arguments `(args)`.
    pub fn args(&self) -> Option<&'a Tuple> {
        self.data.args.as_ref()
    }

    /// The `{value}` group entries.
    pub fn value(&self) -> Option<&'a Group> {
        self.data.value.as_ref()
    }

    /// The body view (data, content, id) of this element.
    pub fn body(&self) -> BodyView<'a> {
        BodyView {
            data: self.data,
            content: self.content,
            id: self.id,
        }
    }

    /// Converts this borrowed view into an owned [`Element`] by cloning.
    pub fn to_element(&self) -> Element {
        Element {
            name: self.name.clone(),
            placement: self.placement,
            data: self.data.clone(),
            content: self.content.map(|c| c.to_vec()),
            id: self.id.cloned(),
            connects: self.connects.to_vec(),
            span: self.span,
        }
    }
}

impl<'a> From<&'a Element> for ElementView<'a> {
    fn from(el: &'a Element) -> Self {
        el.as_view()
    }
}

/// A zero-copy borrowed view of a node's body: its data (`(args)` and `{value}`),
/// optional child `[content]`, and optional `#(id)`.
///
/// Shared by nodes that carry body groups ([`ElementView`], [`Declaration`],
/// [`Connect`], [`Hole`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyView<'a> {
    pub data: &'a Data,
    pub content: Option<&'a [Block]>,
    pub id: Option<&'a Id>,
}

impl<'a> BodyView<'a> {
    /// Positional or keyword arguments `(args)`.
    pub fn args(&self) -> Option<&'a Tuple> {
        self.data.args.as_ref()
    }

    /// The `{value}` group entries.
    pub fn value(&self) -> Option<&'a Group> {
        self.data.value.as_ref()
    }
}

/// One entry inside an element's `{value}` group: either a `key: value`
/// pair or a nested element.
///
/// `Pair` is a named entry (`key: value`), which lets the data view be
/// rebuilt as a [`Tuple`] from a group without copying any other structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    Pair(String, Value),
    /// A named entry, `@name(...)`.
    Element(Element),
    /// A `<?matcher(...)>` hole standing as an entry. See [`Hole`].
    Hole(Hole),
}

/// The entries of a `{...}` group, in source order.
///
/// `{...}` is always data: it parses uniformly into [`Entry`] items in
/// source order, whatever the element is called. "Is this a data group or a
/// list of children?" is not a parse-time branch -- it is a view
/// computed downstream in `tomet-semantics`, which is what lets the parser
/// build the tree without consulting any element vocabulary.
///
/// `Group` holds a source-order sequence of entries (`Group(pub Vec<Entry>)`)
/// and provides named-entry operations exclusively for [`Entry::Pair`] entries.
/// Non-pair entries ([`Entry::Element`], [`Entry::Hole`]) are
/// handled directly by callers through exhaustive pattern matching.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Group(pub Vec<Entry>);

impl Group {
    /// Builds a group from the named entries of `tuple`.
    pub fn from_map(tuple: Tuple) -> Self {
        Group(tuple.into_named().map(|(k, v)| Entry::Pair(k, v)).collect())
    }

    /// The data view: this group's `key: value` pairs, in source order,
    /// as a [`Tuple`]. Non-pair entries are skipped.
    pub fn as_data(&self) -> Tuple {
        let Group(entries) = self;
        Tuple::from_named(entries.iter().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k.clone(), v.clone())),
            Entry::Element(_) | Entry::Hole(_) => None,
        }))
    }

    /// This group's `key: value` pairs, in source order.
    pub fn pairs(&self) -> impl Iterator<Item = (&String, &Value)> {
        let Group(entries) = self;
        entries.iter().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k, v)),
            Entry::Element(_) | Entry::Hole(_) => None,
        })
    }

    /// Mutable counterpart of [`Group::pairs`].
    pub fn pairs_mut(&mut self) -> impl Iterator<Item = (&mut String, &mut Value)> {
        let Group(entries) = self;
        entries.iter_mut().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k, v)),
            Entry::Element(_) | Entry::Hole(_) => None,
        })
    }

    /// The value bound to `key`, if this group has such a pair.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.pairs().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// Mutable counterpart of [`Group::get`].
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.pairs_mut().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// Appends a `key: value` pair.
    pub fn push_pair(&mut self, key: impl Into<String>, value: Value) {
        let Group(entries) = self;
        entries.push(Entry::Pair(key.into(), value));
    }

    /// Removes the first pair bound to `key` and returns its value.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let Group(entries) = self;
        let idx = entries
            .iter()
            .position(|e| matches!(e, Entry::Pair(k, _) if k == key))?;
        match entries.remove(idx) {
            Entry::Pair(_, v) => Some(v),
            Entry::Element(_) | Entry::Hole(_) => {
                unreachable!("position matched a Pair")
            }
        }
    }
}

/// An interpolation: an expression to be evaluated where it stands. It is written in one of three
/// spellings, told apart by what the expression is ([`InterpSpelling`]): `{$ expr }` for any
/// expression, `$(ref)` for one reference, `$name(args)` for one call.
///
/// It is not an [`Element`]: it has no name, no `(args)` / `[content]` and no
/// `#(id)`, and it never takes a `::connect`. It holds an expression and the
/// span of the whole `$`-form, and nothing else. It sits in three places, as
/// [`Block::Interp`], [`Inline::Interp`] and [`Value::Interp`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interpolation {
    pub expr: InterpExpr,
    pub span: Span,
}

/// Displays the interpolation in its canonical spelling, the one [`InterpExpr::spelling`] says: the
/// source text that reads back as the same expression. The three forms are one AST, so a document
/// that spelled `{$ x }` where `$(x)` is canonical is printed as `$(x)`.
impl fmt::Display for Interpolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.expr.write_interpolation(f)
    }
}

/// The three ways an interpolation is written, each for one kind of expression. The spelling is not
/// kept in the AST: it follows from the expression, which is what makes the printed form canonical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpSpelling {
    /// `$name(args)`: one call whose callee is a plain name (`include`, `macro.gh`), or an `addr`.
    Call,
    /// `$(ref)`: one reference -- a name, `#id`, `doc#id`, or a `.member` / `::connect` chain on
    /// those -- or a place number, `$(1)`.
    Reference,
    /// `{$ expr }`: anything else, which is where operators, lambdas, literals and groups live.
    Gate,
}

/// One node of an interpolation's parsed expression tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterpExpr {
    pub kind: InterpExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InterpExprKind {
    Identifier(String),
    Id(String),
    Literal(tomet_scalar::Scalar),
    Call {
        callee: Box<InterpExpr>,
        args: Vec<InterpExpr>,
    },
    /// `object.member`: a named entry of the data an element, a tuple or a section holds, or the start of a
    /// method call (`object.member(...)` is [`InterpExprKind::Call`] on this). A node's own
    /// attributes are not read with a dot; see [`InterpExprKind::Connect`].
    Member {
        object: Box<InterpExpr>,
        member: String,
    },
    /// `object::member`: from a node to what hangs off it -- an attribute of the node (`::id`,
    /// `::name`, a document's `::path`), the connect of that name (`::meta`), or the document or
    /// directory it leads to. The dot reads data inside a node; the double colon goes from node to node.
    Connect {
        object: Box<InterpExpr>,
        member: String,
    },
    NamedArg {
        name: String,
        value: Box<InterpExpr>,
    },
    /// `lhs op rhs`: `+`, `-`, a comparison, `&&` or `||`; see [`BinaryOp`] for the precedence.
    Binary {
        op: BinaryOp,
        lhs: Box<InterpExpr>,
        rhs: Box<InterpExpr>,
    },
    /// `doc#id` -- the element with that unique id inside `doc`, where `doc`
    /// is any expression that yields a document. The leading form `#id`
    /// (no document on the left) is [`InterpExprKind::Id`] and means the
    /// same thing with `self` on the left.
    DocId {
        doc: Box<InterpExpr>,
        id: String,
    },
    /// `addr(tmt:docs/a.tmt#intro)` or `addr("...")` -- an address written as
    /// text. `raw` records the spelling the author used, so printing gives
    /// it back: the raw form reads everything up to the matching `)` without
    /// treating `:`, `.`, `-` or `/` as expression symbols.
    Addr {
        text: String,
        raw: bool,
    },
    /// `( expr )`. Kept as a node, not dropped, so printing a parsed
    /// expression gives back the parentheses the author wrote.
    Group(Box<InterpExpr>),
    /// `()` (empty), `(a,)` (one item), or `(a, b, ...)` (two or more items). Kept as a sequence
    /// expression. `(a)` with no comma is [`InterpExprKind::Group`].
    Seq(Vec<InterpExpr>),
    /// `x => body` or `(a, b) => body`. The body is one expression.
    Lambda {
        params: Vec<String>,
        body: Box<InterpExpr>,
    },
}

/// A binary operator in an expression.
///
/// `+` and `-` are written with white space on both sides: `-` is also part of a name
/// (`daily-note`), so `a-b` is one name and `a - b` is a subtraction. The other arithmetic is
/// functions (`mul`, `div`, `mod`), and there is no `!` (`not`). `&&` and `||` are the only
/// operators that may skip their right-hand side, which is why they are symbols and not functions.
///
/// Precedence, tightest first: the postfix forms, additive, comparison, `&&`, `||`, then `=>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    /// Every operator, so that a reader can look for the one spelled at a position.
    pub const ALL: [BinaryOp; 10] = [
        BinaryOp::Add,
        BinaryOp::Sub,
        BinaryOp::Eq,
        BinaryOp::Ne,
        BinaryOp::Lt,
        BinaryOp::Le,
        BinaryOp::Gt,
        BinaryOp::Ge,
        BinaryOp::And,
        BinaryOp::Or,
    ];

    /// The spelling in source. It is the only place an operator is spelled: the parser finds the
    /// operator at a position by these symbols, longest first (`<=` over `<`).
    pub fn symbol(self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Le => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        }
    }

    /// Whether this is `+` or `-`, which chain to the left and need white space around them.
    pub fn is_additive(self) -> bool {
        matches!(self, BinaryOp::Add | BinaryOp::Sub)
    }

    /// Whether this is one of the six comparisons, which do not chain.
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        )
    }
}

impl InterpExpr {
    /// The interpolation of this expression as source, in the spelling [`InterpExpr::spelling`]
    /// says. For when there is only the expression and no [`Interpolation`] around it.
    pub fn interpolation_source(&self) -> String {
        struct Source<'a>(&'a InterpExpr);
        impl fmt::Display for Source<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.write_interpolation(f)
            }
        }
        Source(self).to_string()
    }

    fn write_interpolation(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.spelling() {
            InterpSpelling::Call => write!(f, "${self}"),
            InterpSpelling::Reference => write!(f, "$({self})"),
            InterpSpelling::Gate => write!(f, "{{$ {self} }}"),
        }
    }

    /// How an interpolation of this expression is written; see [`InterpSpelling`].
    pub fn spelling(&self) -> InterpSpelling {
        match &self.kind {
            InterpExprKind::Addr { .. } => InterpSpelling::Call,
            InterpExprKind::Call { callee, .. } if callee.is_name_chain() => InterpSpelling::Call,
            InterpExprKind::Literal(tomet_scalar::Scalar::Int(n)) if *n >= 0 => InterpSpelling::Reference,
            _ if self.is_reference() => InterpSpelling::Reference,
            _ => InterpSpelling::Gate,
        }
    }

    /// A name, or `.member`s of one: what `$name(...)` can take as its callee.
    fn is_name_chain(&self) -> bool {
        match &self.kind {
            InterpExprKind::Identifier(_) => true,
            InterpExprKind::Member { object, .. } => object.is_name_chain(),
            _ => false,
        }
    }

    /// A name, `#id`, or a `.member`, `::connect` or `#id` on one: what `$(...)` can hold.
    fn is_reference(&self) -> bool {
        match &self.kind {
            InterpExprKind::Identifier(_) | InterpExprKind::Id(_) => true,
            InterpExprKind::Member { object, .. } | InterpExprKind::Connect { object, .. } => {
                object.is_reference()
            }
            InterpExprKind::DocId { doc, .. } => doc.is_reference(),
            _ => false,
        }
    }
}

/// The source spelling of an interpolation expression, without the `$`-form around it. To print
/// an interpolation, display the [`Interpolation`], which adds the form its expression calls for
/// (`$name(args)` writes a call with no gate at all).
///
/// This belongs to the node rather than to any consumer. It was written
/// five times -- once in the printer and once in each of the four convert
/// crates -- and they disagreed: the converters escaped a string literal
/// with Rust's `{:?}` and the printer wrapped it in bare quotes, so a
/// string containing `"` printed back as source that will not parse.
///
/// Escaping here is exactly what `parse_quoted` reads back: `"`, `\`,
/// newline and tab. Not `{:?}`, which also emits `\r` and `\u{...}`
/// escapes the parser does not know -- it would turn a carriage return
/// into the letter `r`.
impl fmt::Display for InterpExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            InterpExprKind::Identifier(name) => f.write_str(name),
            InterpExprKind::Id(name) => write!(f, "#{name}"),
            InterpExprKind::Literal(scalar) => match scalar {
                tomet_scalar::Scalar::Int(i) => write!(f, "{i}"),
                tomet_scalar::Scalar::Float(x) => write!(f, "{x}"),
                tomet_scalar::Scalar::Number(s) => write!(f, "{s}"),
                tomet_scalar::Scalar::Text(s) => write_quoted(f, s),
                tomet_scalar::Scalar::Bool(b) => write!(f, "{b}"),
                tomet_scalar::Scalar::None => f.write_str("none"),
                tomet_scalar::Scalar::Auto => f.write_str("auto"),
            },
            InterpExprKind::Call { callee, args } => {
                write!(f, "{callee}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(")")
            }
            InterpExprKind::Member { object, member } => write!(f, "{object}.{member}"),
            InterpExprKind::Connect { object, member } => write!(f, "{object}::{member}"),
            InterpExprKind::NamedArg { name, value } => write!(f, "{name}: {value}"),
            InterpExprKind::Binary { op, lhs, rhs } => {
                write!(f, "{lhs} {} {rhs}", op.symbol())
            }
            InterpExprKind::DocId { doc, id } => write!(f, "{doc}#{id}"),
            InterpExprKind::Addr { text, raw } => {
                if *raw && addr_text_is_safe_raw(text) {
                    write!(f, "addr({text})")
                } else {
                    f.write_str("addr(")?;
                    write_quoted(f, text)?;
                    f.write_str(")")
                }
            }
            InterpExprKind::Group(inner) => write!(f, "({inner})"),
            InterpExprKind::Seq(items) => match items.as_slice() {
                [] => f.write_str("()"),
                [only] => write!(f, "({only},)"),
                items => {
                    f.write_str("(")?;
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{item}")?;
                    }
                    f.write_str(")")
                }
            },
            InterpExprKind::Lambda { params, body } => {
                match params.as_slice() {
                    [only] => f.write_str(only)?,
                    many => write!(f, "({})", many.join(", "))?,
                }
                write!(f, " => {body}")
            }
        }
    }
}

/// A string literal as `parse_quoted` reads it back.
fn write_quoted(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    f.write_str("\"")?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\t' => f.write_str("\\t")?,
            other => f.write_char(other)?,
        }
    }
    f.write_str("\"")
}

/// Whether `addr(` can be written back in its raw form: the text must read
/// back as the same text, which means no line break, balanced parentheses,
/// no surrounding space and no leading quote.
fn addr_text_is_safe_raw(text: &str) -> bool {
    if text.is_empty() || text.trim() != text || text.starts_with('"') || text.contains('\n') {
        return false;
    }
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    depth == 0
}

/// A Tomet file: its `::name(...)` declarations in the preamble header, followed by its body.
///
/// For prose documents (`.tmt`), `body` defaults to `Vec<Block>`.
/// For structured data files (`.ctmt`), `body` is [`Value`].
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct File<B = Vec<Block>> {
    /// The `::name(...)` declarations at the top of the file, in source order.
    pub header: Vec<Declaration>,
    /// The main file content (blocks for prose, or a data value for cotomet).
    pub body: B,
    pub span: Span,
}

impl<B> File<B> {
    pub fn new(header: Vec<Declaration>, body: B, span: Span) -> Self {
        Self { header, body, span }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Int(n)
    }

    fn tuple(entries: impl IntoIterator<Item = (Option<&'static str>, i64)>) -> Tuple {
        Tuple::try_from_entries(entries.into_iter().map(|(name, n)| match name {
            Some(name) => TupleEntry::named(name, int(n)),
            None => TupleEntry::unnamed(int(n)),
        }))
        .unwrap()
    }

    #[test]
    fn a_key_left_out_and_the_same_key_written_are_one_tuple() {
        // `(1, 2)` and `("0": 1, "1": 2)`.
        let unnamed = tuple([(None, 1), (None, 2)]);
        let written = tuple([(Some("0"), 1), (Some("1"), 2)]);
        assert_eq!(unnamed, written);
        assert!(written.has_no_named());
        assert_eq!(serde_json::to_string(&written).unwrap(), "[1,2]");
        // `{"0": 1}` and `[1]` read as the same tuple.
        let object: Tuple = serde_json::from_str(r#"{"0": 1}"#).unwrap();
        let array: Tuple = serde_json::from_str("[1]").unwrap();
        assert_eq!(object, array);
    }

    #[test]
    fn the_same_keys_in_another_order_are_another_tuple() {
        // `("1": 2, "0": 1)`: the first entry is named `1`, the second is position 0.
        let reordered = tuple([(Some("1"), 2), (Some("0"), 1)]);
        assert_ne!(reordered, tuple([(None, 1), (None, 2)]));
        assert_eq!(reordered.nth(0).unwrap().name(), Some("1"));
        assert_eq!(reordered.nth(1).unwrap().name(), None);
        assert_eq!(reordered.get("0"), Some(&int(1)));
        assert_eq!(reordered.get("1"), Some(&int(2)));
        assert_eq!(
            serde_json::to_string(&reordered).unwrap(),
            r#"{"1":2,"0":1}"#
        );
    }

    #[test]
    fn a_key_is_held_once() {
        let duplicate = |entries: Vec<(Option<&'static str>, i64)>| {
            Tuple::try_from_entries(entries.into_iter().map(|(name, n)| match name {
                Some(name) => TupleEntry::named(name, int(n)),
                None => TupleEntry::unnamed(int(n)),
            }))
            .unwrap_err()
        };
        // An entry named `0` and then an unnamed one: the first IS position 0, so the second is
        // position 1. (Source text never gets here in that shape: the reader gives every entry
        // written without a key its number first, and refuses `("0": 1, 2)` as writing `0` twice.)
        assert_eq!(
            tuple([(Some("0"), 1), (None, 2)]),
            tuple([(None, 1), (None, 2)])
        );
        // An unnamed entry and then one named `0`: position 0 twice.
        let e = duplicate(vec![(None, 1), (Some("0"), 2)]);
        assert_eq!((e.key.as_str(), e.index), ("0", 1));
        // `("1": 1, 2, 3)`: the third entry is position 1.
        let e = duplicate(vec![(Some("1"), 1), (None, 2), (None, 3)]);
        assert_eq!((e.key.as_str(), e.index), ("1", 2));
        let e = duplicate(vec![(Some("a"), 1), (Some("a"), 2)]);
        assert_eq!((e.key.as_str(), e.index), ("a", 1));
        // Text that is not how a position is written is only a name.
        let padded = tuple([(Some("00"), 1), (None, 2), (Some("404"), 3)]);
        assert_eq!(padded.get("00"), Some(&int(1)));
        assert_eq!(padded.get("0"), Some(&int(2)));
        assert_eq!(padded.get("404"), Some(&int(3)));
    }

    #[test]
    fn removing_an_unnamed_entry_moves_the_ones_after_it() {
        let mut t = tuple([(None, 1), (Some("k"), 9), (None, 2), (None, 3)]);
        assert_eq!(t.remove("0"), Some(int(1)));
        assert_eq!(t, tuple([(Some("k"), 9), (None, 2), (None, 3)]));
        assert_eq!(t.get("0"), Some(&int(2)));
        assert_eq!(t.remove("k"), Some(int(9)));
        assert!(t.has_no_named());
        let keys: Vec<String> = t.keyed().map(|(key, _)| key.into_owned()).collect();
        assert_eq!(keys, ["0", "1"]);
    }

    #[test]
    fn test_element_view_and_list_item_as_element_view() {
        let name = Name::bare("item");
        let args_tuple = Tuple::from_unnamed([Value::Int(42)]);
        let data = Data {
            args: Some(args_tuple.clone()),
            value: Some(Group::from_map(Tuple::from_named([(
                "key".to_string(),
                Value::String("val".to_string()),
            )]))),
        };
        let content_blocks = vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Text(Text::new("hello", Span::dummy()))],
            Span::dummy(),
        ))];
        let id = Id("my-id".to_string());
        let connects = vec![Connect {
            name: Name::bare("note"),
            data: Data::default(),
            content: None,
            span: Span::dummy(),
        }];

        let element = Element {
            name: name.clone(),
            placement: Placement::Block,
            data: data.clone(),
            content: Some(content_blocks.clone()),
            id: Some(id.clone()),
            connects: connects.clone(),
            span: Span::dummy(),
        };

        let item = ListItem {
            name: Some(name.clone()),
            data: data.clone(),
            content: content_blocks.clone(),
            id: Some(id.clone()),
            connects: connects.clone(),
            sublist: None,
            span: Span::dummy(),
        };

        let el_view = element.as_view();
        let item_view = item
            .as_element_view()
            .expect("combine item has ElementView");

        assert_eq!(el_view, item_view);
        assert_eq!(el_view.name, &name);
        assert_eq!(el_view.placement, Placement::Block);
        assert_eq!(el_view.args(), Some(&args_tuple));
        assert!(el_view.value().is_some());
        assert_eq!(el_view.id, Some(&id));
        assert_eq!(el_view.connects, &connects);

        // Body view matches
        let body = el_view.body();
        assert_eq!(body, item.as_body_view());
        assert_eq!(body, element.as_body_view());
        assert_eq!(body.args(), Some(&args_tuple));
        assert_eq!(body.content, Some(content_blocks.as_slice()));
        assert_eq!(body.id, Some(&id));

        // Converting back to Element produces exact equivalent
        assert_eq!(el_view.to_element(), element);
        assert_eq!(item_view.to_element(), element);

        // Plain list item without a name has no ElementView, but has BodyView
        let plain_item = ListItem {
            name: None,
            data: data.clone(),
            content: content_blocks.clone(),
            id: Some(id.clone()),
            connects: connects.clone(),
            sublist: None,
            span: Span::dummy(),
        };
        assert!(plain_item.as_element_view().is_none());
        assert_eq!(plain_item.as_body_view(), body);

        // BodyView on Declaration, Hole, Connect
        let decl = Declaration {
            name: name.clone(),
            data: data.clone(),
            content: Some(content_blocks.clone()),
            id: Some(id.clone()),
            span: Span::dummy(),
        };
        assert_eq!(decl.as_body_view(), body);

        let hole = Hole {
            matcher: Some(name.clone()),
            placement: Placement::Block,
            data: data.clone(),
            content: Some(content_blocks.clone()),
            span: Span::dummy(),
        };
        assert_eq!(
            hole.as_body_view(),
            BodyView {
                data: &data,
                content: Some(content_blocks.as_slice()),
                id: None,
            }
        );

        let connect = Connect {
            name,
            data: data.clone(),
            content: Some(content_blocks.clone()),
            span: Span::dummy(),
        };
        assert_eq!(
            connect.as_body_view(),
            BodyView {
                data: &data,
                content: Some(content_blocks.as_slice()),
                id: None,
            }
        );
    }

    fn name_expr(name: &str) -> InterpExpr {
        InterpExpr {
            kind: InterpExprKind::Identifier(name.to_string()),
            span: Span::dummy(),
        }
    }

    fn int_expr(n: i64) -> InterpExpr {
        InterpExpr {
            kind: InterpExprKind::Literal(tomet_scalar::Scalar::Int(n)),
            span: Span::dummy(),
        }
    }

    fn call_expr(name: &str, args: Vec<InterpExpr>) -> InterpExpr {
        InterpExpr {
            kind: InterpExprKind::Call {
                callee: Box::new(name_expr(name)),
                args,
            },
            span: Span::dummy(),
        }
    }

    fn seat(expr: InterpExpr) -> TypeSeat {
        TypeSeat {
            expr,
            span: Span::dummy(),
        }
    }

    #[test]
    fn a_type_seat_displays_as_the_source_it_reads_back_from() {
        // A bare name and a call with no arguments are two written forms, and stay two.
        assert_eq!(seat(name_expr("uuid")).to_string(), "<$uuid>");
        assert_eq!(seat(call_expr("uuid", vec![])).to_string(), "<$uuid()>");

        let named = |name: &str, value: InterpExpr| InterpExpr {
            kind: InterpExprKind::NamedArg {
                name: name.to_string(),
                value: Box::new(value),
            },
            span: Span::dummy(),
        };
        let list = call_expr(
            "list",
            vec![name_expr("string"), named("length", int_expr(6))],
        );
        assert_eq!(seat(list).to_string(), "<$list(string, length: 6)>");
        let date = call_expr("date", vec![named("default", name_expr("auto"))]);
        assert_eq!(seat(date).to_string(), "<$date(default: auto)>");
    }

    #[test]
    fn test_tuple_and_value_json_serde() {
        // Empty tuple serializes to `{}`
        let empty = Tuple::new();
        assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");
        assert_eq!(
            serde_json::to_string(&Value::Tuple(empty.clone())).unwrap(),
            "{}"
        );

        // All unnamed entries serialize to array
        let unnamed = Tuple::from_unnamed([Value::Int(1), Value::Int(2)]);
        assert_eq!(serde_json::to_string(&unnamed).unwrap(), "[1,2]");
        assert_eq!(
            serde_json::to_string(&Value::Tuple(unnamed.clone())).unwrap(),
            "[1,2]"
        );

        // All named entries serialize to object
        let named = Tuple::from_named([
            ("a".to_string(), Value::Int(1)),
            ("b".to_string(), Value::Int(2)),
        ]);
        assert_eq!(serde_json::to_string(&named).unwrap(), r#"{"a":1,"b":2}"#);
        assert_eq!(
            serde_json::to_string(&Value::Tuple(named)).unwrap(),
            r#"{"a":1,"b":2}"#
        );

        // Mixed serialize to object with unnamed entries keyed by their index among unnamed
        let mut mixed = Tuple::new();
        mixed.try_push_unnamed(Value::Int(10)).unwrap();
        mixed.try_push_named("foo", Value::Int(20)).unwrap();
        mixed.try_push_unnamed(Value::Int(30)).unwrap();
        assert_eq!(
            serde_json::to_string(&mixed).unwrap(),
            r#"{"0":10,"foo":20,"1":30}"#
        );
        assert_eq!(
            serde_json::to_string(&Value::Tuple(mixed)).unwrap(),
            r#"{"0":10,"foo":20,"1":30}"#
        );

        // Deserialization: [] and {} both deserialize to empty tuple
        let from_empty_obj: Value = serde_json::from_str("{}").unwrap();
        match from_empty_obj {
            Value::Tuple(t) => assert!(t.is_empty()),
            other => panic!("expected Tuple, got {other:?}"),
        }
        let from_empty_arr: Value = serde_json::from_str("[]").unwrap();
        match from_empty_arr {
            Value::Tuple(t) => assert!(t.is_empty()),
            other => panic!("expected Tuple, got {other:?}"),
        }

        // Deserialization: array yields unnamed entries
        let from_arr: Value = serde_json::from_str("[1, 2]").unwrap();
        match from_arr {
            Value::Tuple(t) => {
                assert_eq!(t.len(), 2);
                assert!(t.has_no_named());
                assert_eq!(t.nth_unnamed(0), Some(&Value::Int(1)));
                assert_eq!(t.nth_unnamed(1), Some(&Value::Int(2)));
            }
            other => panic!("expected Tuple, got {other:?}"),
        }

        // Deserialization: object yields named entries
        let from_obj: Value = serde_json::from_str(r#"{"a": 1}"#).unwrap();
        match from_obj {
            Value::Tuple(t) => {
                assert_eq!(t.len(), 1);
                assert!(t.has_no_unnamed());
                assert_eq!(t.get("a"), Some(&Value::Int(1)));
            }
            other => panic!("expected Tuple, got {other:?}"),
        }

        // Call serialization with Tuple fields
        let call = Value::Call(
            "my_fn".to_string(),
            Tuple::from_unnamed([Value::Int(1), Value::Int(2)]),
        );
        assert_eq!(
            serde_json::to_string(&call).unwrap(),
            r#"{"call":"my_fn","args":[1,2]}"#
        );

        let call_empty = Value::Call("empty_fn".to_string(), Tuple::new());
        assert_eq!(
            serde_json::to_string(&call_empty).unwrap(),
            r#"{"call":"empty_fn","args":{}}"#
        );

        // Deserialization: ["a"] and {"0": "a"} read as the same tuple
        let from_arr: Value = serde_json::from_str(r#"["a"]"#).unwrap();
        let from_obj: Value = serde_json::from_str(r#"{"0": "a"}"#).unwrap();
        assert_eq!(from_arr, from_obj);

        // AST equality: (a, b) and ("0": a, "1": b) are the same tree
        let unnamed_tuple =
            Tuple::from_unnamed([Value::String("a".into()), Value::String("b".into())]);
        let named_tuple = Tuple::from_named([
            ("0".into(), Value::String("a".into())),
            ("1".into(), Value::String("b".into())),
        ]);
        assert_eq!(unnamed_tuple, named_tuple);
    }

    #[test]
    fn generic_tuple_preserves_rules_for_custom_type() {
        let unnamed: Tuple<i32> = Tuple::from_unnamed([10, 20]);
        let canonical_named: Tuple<i32> =
            Tuple::try_from_entries([TupleEntry::named("0", 10), TupleEntry::named("1", 20)])
                .unwrap();
        assert_eq!(unnamed, canonical_named);
        assert!(canonical_named.has_no_named());
        assert_eq!(unnamed.get("0"), Some(&10));
        assert_eq!(unnamed.get("1"), Some(&20));

        let mapped = unnamed.map_values(|n| n * 2);
        assert_eq!(mapped.get("0"), Some(&20));
        assert_eq!(mapped.get("1"), Some(&40));
    }
}
