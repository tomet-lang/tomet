//! The data model of a value: [`Value`], and the [`Tuple`] of [`TupleEntry`]s that holds named and unnamed values together.

use crate::{Block, Element, Hole, Interpolation, TypeSeat};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A pure data value: the subset of Tomet with a direct serde
/// equivalent (no headings, prose, or links).
///
/// A scalar value is held in [`Value::Scalar`], wrapping [`tomet_scalar::Scalar`].
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A scalar value: `none`, `auto`, a boolean, an integer, a float,
    /// a number kept as written, or text. See [`tomet_scalar::Scalar`].
    Scalar(tomet_scalar::Scalar),
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
    span: tomet_span::Span,
}

impl<V> Default for Tuple<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            span: tomet_span::Span::dummy(),
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
            span: tomet_span::Span::dummy(),
        }
    }

    /// The source span of this tuple (or of the [`Value::Call`] that holds it).
    pub fn span(&self) -> tomet_span::Span {
        self.span
    }

    /// Sets the source span of this tuple.
    pub fn set_span(&mut self, span: tomet_span::Span) {
        self.span = span;
    }

    /// Returns this tuple with its source span set to `span`.
    pub fn with_span(mut self, span: tomet_span::Span) -> Self {
        self.span = span;
        self
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
            span: self.span,
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
        Ok(Tuple {
            entries,
            span: self.span,
        })
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
    /// The scalar this value is, if it is one.
    pub fn as_scalar(&self) -> Option<&tomet_scalar::Scalar> {
        match self {
            Value::Scalar(s) => Some(s),
            _ => None,
        }
    }
}

impl From<tomet_scalar::Scalar> for Value {
    fn from(s: tomet_scalar::Scalar) -> Self {
        Value::Scalar(s)
    }
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::Scalar(s) => match s {
                tomet_scalar::Scalar::None => serializer.serialize_none(),
                tomet_scalar::Scalar::Auto => Err(serde::ser::Error::custom(
                    "`auto` has no JSON representation",
                )),
                tomet_scalar::Scalar::Bool(b) => serializer.serialize_bool(*b),
                tomet_scalar::Scalar::Int(i) => serializer.serialize_i64(*i),
                tomet_scalar::Scalar::Float(f) => serializer.serialize_f64(*f),
                // A number on the wire is an f64: that is the most a serializer
                // that has no arbitrary-precision number can say. One that cannot
                // be one (too large) is sent as the text it was written as.
                tomet_scalar::Scalar::Number(text) => match text.parse::<f64>() {
                    Ok(f) if f.is_finite() => serializer.serialize_f64(f),
                    _ => serializer.serialize_str(text),
                },
                tomet_scalar::Scalar::Text(s) => serializer.serialize_str(s),
            },
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
                Ok(Value::Scalar(tomet_scalar::Scalar::Bool(v)))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
                Ok(Value::Scalar(tomet_scalar::Scalar::Int(v)))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
                Ok(Value::Scalar(tomet_scalar::Scalar::Int(v as i64)))
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> {
                Ok(Value::Scalar(tomet_scalar::Scalar::Float(v)))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Value::Scalar(tomet_scalar::Scalar::Text(v.to_string())))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
                Ok(Value::Scalar(tomet_scalar::Scalar::Text(v)))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Scalar(tomet_scalar::Scalar::None))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Scalar(tomet_scalar::Scalar::None))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Scalar(tomet_scalar::Scalar::Int(n))
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
    fn test_tuple_and_value_json_serde() {
        // Empty tuple serializes to `{}`
        let empty = Tuple::new();
        assert_eq!(serde_json::to_string(&empty).unwrap(), "{}");
        assert_eq!(
            serde_json::to_string(&Value::Tuple(empty.clone())).unwrap(),
            "{}"
        );

        // All unnamed entries serialize to array
        let unnamed = Tuple::from_unnamed([
            Value::Scalar(tomet_scalar::Scalar::Int(1)),
            Value::Scalar(tomet_scalar::Scalar::Int(2)),
        ]);
        assert_eq!(serde_json::to_string(&unnamed).unwrap(), "[1,2]");
        assert_eq!(
            serde_json::to_string(&Value::Tuple(unnamed.clone())).unwrap(),
            "[1,2]"
        );

        // All named entries serialize to object
        let named = Tuple::from_named([
            ("a".to_string(), Value::Scalar(tomet_scalar::Scalar::Int(1))),
            ("b".to_string(), Value::Scalar(tomet_scalar::Scalar::Int(2))),
        ]);
        assert_eq!(serde_json::to_string(&named).unwrap(), r#"{"a":1,"b":2}"#);
        assert_eq!(
            serde_json::to_string(&Value::Tuple(named)).unwrap(),
            r#"{"a":1,"b":2}"#
        );

        // Mixed serialize to object with unnamed entries keyed by their index among unnamed
        let mut mixed = Tuple::new();
        mixed
            .try_push_unnamed(Value::Scalar(tomet_scalar::Scalar::Int(10)))
            .unwrap();
        mixed
            .try_push_named("foo", Value::Scalar(tomet_scalar::Scalar::Int(20)))
            .unwrap();
        mixed
            .try_push_unnamed(Value::Scalar(tomet_scalar::Scalar::Int(30)))
            .unwrap();
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
                assert_eq!(
                    t.nth_unnamed(0),
                    Some(&Value::Scalar(tomet_scalar::Scalar::Int(1)))
                );
                assert_eq!(
                    t.nth_unnamed(1),
                    Some(&Value::Scalar(tomet_scalar::Scalar::Int(2)))
                );
            }
            other => panic!("expected Tuple, got {other:?}"),
        }

        // Deserialization: object yields named entries
        let from_obj: Value = serde_json::from_str(r#"{"a": 1}"#).unwrap();
        match from_obj {
            Value::Tuple(t) => {
                assert_eq!(t.len(), 1);
                assert!(t.has_no_unnamed());
                assert_eq!(
                    t.get("a"),
                    Some(&Value::Scalar(tomet_scalar::Scalar::Int(1)))
                );
            }
            other => panic!("expected Tuple, got {other:?}"),
        }

        // Call serialization with Tuple fields
        let call = Value::Call(
            "my_fn".to_string(),
            Tuple::from_unnamed([
                Value::Scalar(tomet_scalar::Scalar::Int(1)),
                Value::Scalar(tomet_scalar::Scalar::Int(2)),
            ]),
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
        let unnamed_tuple = Tuple::from_unnamed([
            Value::Scalar(tomet_scalar::Scalar::Text("a".into())),
            Value::Scalar(tomet_scalar::Scalar::Text("b".into())),
        ]);
        let named_tuple = Tuple::from_named([
            (
                "0".into(),
                Value::Scalar(tomet_scalar::Scalar::Text("a".into())),
            ),
            (
                "1".into(),
                Value::Scalar(tomet_scalar::Scalar::Text("b".into())),
            ),
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
