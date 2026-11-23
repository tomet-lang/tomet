//! The data model of a value: [`Value`], and the [`Tuple`] of [`TupleEntry`]s that holds named and unnamed values together.

use crate::{Block, Element, Hole, Interpolation, TypeSeat};
use serde::{Deserialize, Serialize};
use std::fmt;

/// What can stand where a value goes: a scalar, a tuple, or a node the parser keeps for a
/// later layer (a call, an interpolation, a hole, a type seat, an element, block content).
///
/// A scalar value is held in [`Value::Scalar`], wrapping [`tomet_scalar::Scalar`].
///
/// =[ Wire ]
///
/// On the wire a `Value` is what every other AST node is: an externally tagged enum, a variant name
/// and its payload, and it reads back as exactly the variant that was written.
/// `Value::Scalar(Scalar::Int(1))` is `{"Scalar":{"Int":1}}`, `Value::Call("enum", args)` is
/// `{"Call":["enum", <Tuple>]}`, and a [`Value::TypeSeat`] and a [`Value::Interp`] are as
/// different on the wire as they are in memory. Nothing is lost and nothing is ambiguous, which is
/// what a document needs that is handed to another process and handed back.
///
/// A data consumer that wants `null`, numbers, strings, arrays and objects, and accepts losing
/// what those cannot say, reads the plain-data projection instead: [`Value::to_plain`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// `key: [ ... ]` -- block content (the same grammar as
    /// an element's own `[content]`/`|content` slot, see
    /// `tmtroot/docs/spec/feature/content-shape.tmt`) sitting as a named
    /// value inside `(args)`/`{data}`, e.g. `@conflict(a: [ ... ], b: [
    /// ... ])`. `[...]` means block content uniformly at any
    /// value position, as it does at the element level; a tuple is
    /// `(a, b)`, and `[a, b]` is not a list literal.
    Blocks(Vec<Block>),
}

/// The key of a [`TupleEntry`] when one was written: a plain/quoted name, or a type seat (`<$type>`).
///
/// On the wire the two kinds stay apart: `{"Name":"a"}` and `{"TypeSeat":{...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EntryKey {
    Name(String),
    TypeSeat(crate::TypeSeat),
}

/// One entry of a [`Tuple`]: a value, with a key or without one.
///
/// An entry with no key is an entry whose key was not written; its key is its position among
/// the unnamed entries of its tuple. An entry does not know that position: ask the tuple
/// ([`Tuple::keyed`], [`Tuple::get`]).
///
/// On the wire an entry is `{"key": null | EntryKey, "value": V}`; `null` is a key that was not written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "TupleEntry"))]
pub struct TupleEntry<V = Value> {
    key: Option<EntryKey>,
    value: V,
}

impl<V> TupleEntry<V> {
    pub fn unnamed(value: V) -> Self {
        Self { key: None, value }
    }

    pub fn named(name: impl Into<String>, value: V) -> Self {
        Self {
            key: Some(EntryKey::Name(name.into())),
            value,
        }
    }

    pub fn type_seat(seat: crate::TypeSeat, value: V) -> Self {
        Self {
            key: Some(EntryKey::TypeSeat(seat)),
            value,
        }
    }

    /// The key this entry is written with, or `None` for an unnamed entry.
    pub fn key(&self) -> Option<&EntryKey> {
        self.key.as_ref()
    }

    /// The name this entry is written with, or `None` for an unnamed or type-seat-keyed entry.
    pub fn name(&self) -> Option<&str> {
        match &self.key {
            Some(EntryKey::Name(name)) => Some(name.as_str()),
            _ => None,
        }
    }

    /// The type seat key this entry is written with, or `None` otherwise.
    pub fn type_seat_key(&self) -> Option<&crate::TypeSeat> {
        match &self.key {
            Some(EntryKey::TypeSeat(seat)) => Some(seat),
            _ => None,
        }
    }

    /// Mutable access to the type seat key, if this entry is keyed by a type seat.
    pub fn type_seat_key_mut(&mut self) -> Option<&mut crate::TypeSeat> {
        match &mut self.key {
            Some(EntryKey::TypeSeat(seat)) => Some(seat),
            _ => None,
        }
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

    pub fn into_parts(self) -> (Option<EntryKey>, V) {
        (self.key, self.value)
    }

    pub fn map_value<U>(self, f: impl FnOnce(V) -> U) -> TupleEntry<U> {
        TupleEntry {
            key: self.key,
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
/// `("0": a, "1": b)` are the same tuple, and `("0": a, b)` writes the key `0` twice, which is
/// an error: a key is held once.
///
/// =[ In Rust ]
///
/// A tuple keeps that true by how it is built, not by asking its callers to:
///
/// - an entry stores a name only when it needs one. A name that is the position the entry would
///   have unnamed is dropped as the entry comes in, so the two spellings above are one value,
///   equal by `==`. No entry stores a position: removing or naming an unnamed entry moves the
///   ones after it by itself.
/// - no key is held twice. Every way to build or change a tuple that could repeat one is
///   fallible and returns [`DuplicateKey`]; nothing else can add an entry.
///
/// =[ Wire ]
///
/// On the wire a tuple is its entries in written order, and its span if it has one (`null` if it is not written anywhere):
/// `{"entries":[{"key":null,"value":...},{"key":{"Name":"k"},"value":...}],"span":{"start":0,"end":9}}`. Every entry
/// keeps the kind of its key (none, a name, a type seat) and its place, so no reader tells a list
/// from a map by looking at the data, and no order is lost to a language whose objects sort their
/// keys. Reading a tuple builds it like any other way of building one does: a name that is the
/// position the entry would have unnamed is the unnamed entry, and a key written twice is an error,
/// not a tuple.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(try_from = "TupleWire<V>", bound(deserialize = "V: Deserialize<'de>"))]
#[cfg_attr(feature = "schema", schemars(rename = "Tuple"))]
pub struct Tuple<V = Value> {
    entries: Vec<TupleEntry<V>>,
    span: Option<tomet_span::Span>,
}

/// What a [`Tuple`] is read from: its fields as written, before the rules of its keys apply.
#[derive(Deserialize)]
#[serde(bound(deserialize = "V: Deserialize<'de>"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
struct TupleWire<V> {
    entries: Vec<TupleEntry<V>>,
    span: Option<tomet_span::Span>,
}

impl<V> TryFrom<TupleWire<V>> for Tuple<V> {
    type Error = DuplicateKey;

    fn try_from(wire: TupleWire<V>) -> Result<Self, DuplicateKey> {
        let mut tuple = Self::try_from_entries(wire.entries)?;
        tuple.span = wire.span;
        Ok(tuple)
    }
}

/// By its entries, in order. A span is where a tuple was written, not what it says, so it does not
/// count (as no [`Span`](tomet_span::Span) does): a parsed tuple equals the one built in code.
impl<V: PartialEq> PartialEq for Tuple<V> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl<V> Default for Tuple<V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            span: None,
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
        let entry = match &entry.key {
            Some(EntryKey::Name(name)) => match as_position(name) {
                // Named with the position it would have unnamed: the same entry, unnamed.
                Some(position) if position == next => TupleEntry::unnamed(entry.value),
                Some(position) if position < next => {
                    return Err(duplicate(name.clone()));
                }
                _ if self.entries.iter().any(|e| e.name() == Some(name.as_str())) => {
                    return Err(duplicate(name.clone()));
                }
                _ => entry,
            },
            Some(EntryKey::TypeSeat(seat)) => {
                if self.entries.iter().any(|e| e.type_seat_key() == Some(seat)) {
                    return Err(duplicate(seat.to_string()));
                }
                entry
            }
            None => entry,
        };
        if entry.key.is_none() {
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
            span: None,
        }
    }

    /// Where this tuple is written in the source (or, for a [`Value::Call`]'s arguments, where the
    /// whole call is), or `None` if it is not written anywhere: a tuple built in code, or put together
    /// by a pass from others.
    pub fn span(&self) -> Option<tomet_span::Span> {
        self.span
    }

    /// Sets the source span of this tuple.
    pub fn set_span(&mut self, span: tomet_span::Span) {
        self.span = Some(span);
    }

    /// Returns this tuple with its source span set to `span`.
    pub fn with_span(mut self, span: tomet_span::Span) -> Self {
        self.span = Some(span);
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
        self.entries.iter().filter(|e| e.key.is_none()).count()
    }

    /// Whether no entry has a key. True for an empty tuple.
    pub fn has_no_named(&self) -> bool {
        self.entries.iter().all(|e| e.key.is_none())
    }

    /// Whether every entry has a key. True for an empty tuple.
    pub fn has_no_unnamed(&self) -> bool {
        self.entries.iter().all(|e| e.key.is_some())
    }

    /// The index, among the entries as written, of the entry whose key is `key`.
    pub fn index_of(&self, key: &str) -> Option<usize> {
        let position = as_position(key);
        let mut seen = 0;
        self.entries.iter().position(|entry| match &entry.key {
            Some(EntryKey::Name(name)) => name == key,
            Some(EntryKey::TypeSeat(_)) => false,
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
            .filter(|e| e.key.is_none())
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

    /// Every entry with its key, in written order: the name of a named entry, the `<$type>`
    /// spelling of a type-seat key, or the position of an unnamed one.
    pub fn keyed(&self) -> impl Iterator<Item = (std::borrow::Cow<'_, str>, &V)> {
        let mut position = 0;
        self.entries.iter().map(move |entry| {
            let key = match &entry.key {
                Some(EntryKey::Name(name)) => std::borrow::Cow::Borrowed(name.as_str()),
                Some(EntryKey::TypeSeat(seat)) => std::borrow::Cow::Owned(seat.to_string()),
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
                    key: e.key,
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
                key: e.key,
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
            .filter_map(|e| Some((e.name()?, &e.value)))
    }

    /// Mutable counterpart of [`Tuple::named`].
    pub fn named_mut(&mut self) -> impl Iterator<Item = (&str, &mut V)> {
        self.entries.iter_mut().filter_map(|e| match &e.key {
            Some(EntryKey::Name(name)) => Some((name.as_str(), &mut e.value)),
            _ => None,
        })
    }

    /// Owning counterpart of [`Tuple::named`].
    pub fn into_named(self) -> impl Iterator<Item = (String, V)> {
        self.entries.into_iter().filter_map(|e| match e.key {
            Some(EntryKey::Name(name)) => Some((name, e.value)),
            _ => None,
        })
    }

    /// The unnamed entries' values, in written order: the `n`-th has the key `n`.
    pub fn unnamed(&self) -> impl Iterator<Item = &V> {
        self.entries
            .iter()
            .filter_map(|e| e.key.is_none().then_some(&e.value))
    }

    /// Owning counterpart of [`Tuple::unnamed`].
    pub fn into_unnamed(self) -> impl Iterator<Item = V> {
        self.entries
            .into_iter()
            .filter_map(|e| e.key.is_none().then_some(e.value))
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
        entries[index].key = Some(EntryKey::Name(name.into()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Data, Element, Hole, Interpolation, Name, Paragraph, Placement};
    use serde_json::json;
    use tomet_span::Span;

    fn int(n: i64) -> Value {
        Value::Scalar(tomet_scalar::Scalar::Int(n))
    }

    fn text(s: &str) -> Value {
        Value::Scalar(tomet_scalar::Scalar::Text(s.into()))
    }

    fn ident(name: &str) -> crate::InterpExpr {
        crate::InterpExpr {
            kind: crate::InterpExprKind::Identifier(name.into()),
            span: Span::dummy(),
        }
    }

    fn seat(name: &str) -> TypeSeat {
        TypeSeat {
            expr: ident(name),
            span: Span::dummy(),
        }
    }

    /// The wire form of `value`, as a JSON tree.
    fn wire<T: Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value).unwrap()
    }

    /// `value` written and read back: equal, and writing it again says the same thing (which also
    /// holds what `==` does not: a [`Span`] is equal to every other).
    fn round_trip<T: Serialize + serde::de::DeserializeOwned + PartialEq + fmt::Debug>(
        value: &T,
    ) -> T {
        let written = serde_json::to_string(value).unwrap();
        let read: T = serde_json::from_str(&written).unwrap();
        assert_eq!(&read, value);
        assert_eq!(serde_json::to_string(&read).unwrap(), written);
        read
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
            reordered.nth(0).unwrap().key(),
            Some(&EntryKey::Name("1".into()))
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
    fn two_spellings_of_one_tuple_are_one_wire() {
        let unnamed = Value::Tuple(Tuple::from_unnamed([int(1), int(2)]));
        let written = Value::Tuple(Tuple::from_named([
            ("0".to_string(), int(1)),
            ("1".to_string(), int(2)),
        ]));
        assert_eq!(wire(&unnamed), wire(&written));
        assert_eq!(
            wire(&unnamed),
            json!({"Tuple": {"entries": [
                {"key": null, "value": {"Scalar": {"Int": 1}}},
                {"key": null, "value": {"Scalar": {"Int": 2}}},
            ], "span": null}})
        );
        // A name that is the position its entry would have unnamed is read as the unnamed entry.
        let read: Value = serde_json::from_value(json!({"Tuple": {"entries": [
            {"key": {"Name": "0"}, "value": {"Scalar": {"Int": 1}}},
            {"key": {"Name": "1"}, "value": {"Scalar": {"Int": 2}}},
        ], "span": null}}))
        .unwrap();
        assert_eq!(read, unnamed);
        assert_eq!(wire(&read), wire(&unnamed));
    }

    #[test]
    fn a_tuple_keeps_the_order_and_the_kind_of_its_keys() {
        // `(10, foo: 20, 30)`: the order of mixed entries survives, and so does which have a key.
        let mixed = Tuple::try_from_entries([
            TupleEntry::unnamed(int(10)),
            TupleEntry::named("foo", int(20)),
            TupleEntry::unnamed(int(30)),
        ])
        .unwrap();
        let keys: Vec<serde_json::Value> = wire(&mixed)["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["key"].clone())
            .collect();
        assert_eq!(keys, [json!(null), json!({"Name": "foo"}), json!(null)]);
        round_trip(&mixed);

        // `("1": 2, "0": 1)` is not `(2, 1)`'s lookalike `(1, 2)`: the first entry is named.
        let reordered = tuple([(Some("1"), 2), (Some("0"), 1)]);
        assert_ne!(reordered, tuple([(None, 1), (None, 2)]));
        assert_ne!(wire(&reordered), wire(&tuple([(None, 1), (None, 2)])));
        round_trip(&reordered);

        // A name that merely looks like a type seat is not a type seat key.
        let name = TupleEntry::named("<$int>", int(1));
        let seat_entry = TupleEntry::type_seat(seat("int"), int(1));
        let by_name = Tuple::from_entries([name]);
        let by_seat = Tuple::from_entries([seat_entry]);
        assert_ne!(by_name, by_seat);
        assert_eq!(
            wire(&by_name)["entries"][0]["key"],
            json!({"Name": "<$int>"})
        );
        assert!(wire(&by_seat)["entries"][0]["key"]["TypeSeat"].is_object());
        assert_eq!(round_trip(&by_seat).nth(0).unwrap().name(), None);
        assert!(
            round_trip(&by_seat)
                .nth(0)
                .unwrap()
                .type_seat_key()
                .is_some()
        );
    }

    #[test]
    fn a_tuple_is_read_with_the_rules_of_its_keys() {
        let entry = |key: serde_json::Value| json!({"key": key, "value": {"Scalar": "None"}});
        let tuple = |entries: Vec<serde_json::Value>| {
            serde_json::from_value::<Tuple>(
                json!({"entries": entries, "span": {"start": 1, "end": 4}}),
            )
        };
        let repeated = tuple(vec![
            entry(json!({"Name": "a"})),
            entry(json!({"Name": "a"})),
        ]);
        assert!(
            repeated
                .unwrap_err()
                .to_string()
                .contains("duplicate key `a`")
        );
        // Position 0 twice: unnamed, then named `0`.
        let repeated = tuple(vec![entry(json!(null)), entry(json!({"Name": "0"}))]);
        assert!(
            repeated
                .unwrap_err()
                .to_string()
                .contains("duplicate key `0`")
        );
        // The span is read, not defaulted.
        let held = tuple(vec![entry(json!(null))]).unwrap();
        assert!(held.span().unwrap().exact_eq(&Span::new(1, 4)));
    }

    #[test]
    fn what_used_to_look_alike_is_told_apart() {
        let args = Tuple::from_unnamed([int(1), int(2)]);

        // A call, and a tuple that spells the keys `call` and `args`.
        let call = Value::Call("enum".into(), args.clone());
        let lookalike = Value::Tuple(
            Tuple::try_from_entries([
                TupleEntry::named("call", text("enum")),
                TupleEntry::named("args", Value::Tuple(args)),
            ])
            .unwrap(),
        );
        assert_ne!(call, lookalike);
        assert_ne!(wire(&call), wire(&lookalike));
        assert_eq!(wire(&call)["Call"][0], json!("enum"));
        assert_eq!(round_trip(&call), call);
        assert_eq!(round_trip(&lookalike), lookalike);

        // An interpolation and a type seat with the same expression and span.
        let interp = Value::Interp(Interpolation {
            expr: ident("int"),
            span: Span::dummy(),
        });
        let type_seat = Value::TypeSeat(seat("int"));
        assert_ne!(wire(&interp), wire(&type_seat));
        assert!(matches!(round_trip(&interp), Value::Interp(_)));
        assert!(matches!(round_trip(&type_seat), Value::TypeSeat(_)));

        // A tuple that spells the keys of an interpolation.
        let spelled = Value::Tuple(
            Tuple::try_from_entries([
                TupleEntry::named("expr", text("int")),
                TupleEntry::named("span", int(0)),
            ])
            .unwrap(),
        );
        assert!(matches!(round_trip(&spelled), Value::Tuple(_)));
        assert_ne!(wire(&spelled), wire(&interp));

        // Block content, an empty one, and the empty tuple.
        let blocks = Value::Blocks(vec![Block::Paragraph(Paragraph {
            content: vec![],
            span: Span::dummy(),
        })]);
        let empty_blocks = Value::Blocks(vec![]);
        let empty_tuple = Value::Tuple(Tuple::new());
        assert_eq!(wire(&empty_blocks), json!({"Blocks": []}));
        assert!(matches!(round_trip(&blocks), Value::Blocks(b) if b.len() == 1));
        assert!(matches!(round_trip(&empty_blocks), Value::Blocks(b) if b.is_empty()));
        assert!(matches!(round_trip(&empty_tuple), Value::Tuple(t) if t.is_empty()));

        // A hole, and an element, each as itself.
        let hole = Value::Hole(Box::new(Hole {
            matcher: Some(Name::bare("m")),
            placement: Placement::Inline,
            data: Data::default(),
            content: None,
            span: Span::dummy(),
        }));
        let element = Value::Element(Box::new(Element {
            name: Name::namespaced("doc", "icon"),
            placement: Placement::Inline,
            data: Data {
                args: Some(Tuple::from_unnamed([text("triangle")])),
                value: None,
            },
            content: None,
            id: None,
            connects: vec![],
            span: Span::dummy(),
        }));
        assert!(matches!(round_trip(&hole), Value::Hole(_)));
        assert_eq!(round_trip(&element), element);
        assert_ne!(wire(&hole), wire(&element));
    }

    #[test]
    fn every_scalar_comes_back_as_itself() {
        use tomet_scalar::Scalar;
        let scalars = [
            (Scalar::None, json!("None")),
            (Scalar::Auto, json!("Auto")),
            (Scalar::Bool(true), json!({"Bool": true})),
            (Scalar::Int(-7), json!({"Int": -7})),
            (Scalar::Float(1.0), json!({"Float": 1.0})),
            (Scalar::Float(0.1), json!({"Float": 0.1})),
            (Scalar::Number("1.10".into()), json!({"Number": "1.10"})),
            (Scalar::Number("1e3".into()), json!({"Number": "1e3"})),
            (Scalar::Number("-0".into()), json!({"Number": "-0"})),
            (
                Scalar::Number("123456789012345678901234567890".into()),
                json!({"Number": "123456789012345678901234567890"}),
            ),
            (Scalar::Text("auto".into()), json!({"Text": "auto"})),
            (Scalar::Text("1.10".into()), json!({"Text": "1.10"})),
        ];
        for (scalar, expected) in scalars {
            let value = Value::Scalar(scalar);
            assert_eq!(wire(&value), json!({ "Scalar": expected }));
            assert_eq!(round_trip(&value), value);
        }
        // An integer and the float that prints like one are not the same value on the wire.
        assert_ne!(
            wire(&Value::Scalar(tomet_scalar::Scalar::Int(1))),
            wire(&Value::Scalar(tomet_scalar::Scalar::Float(1.0)))
        );
    }

    #[test]
    fn a_tuple_of_any_value_type_has_a_wire() {
        let tuple =
            Tuple::try_from_entries([TupleEntry::unnamed(10_i32), TupleEntry::named("k", 20)])
                .unwrap()
                .with_span(Span::new(2, 9));
        assert_eq!(
            wire(&tuple),
            json!({"entries": [
                {"key": null, "value": 10},
                {"key": {"Name": "k"}, "value": 20},
            ], "span": {"start": 2, "end": 9}})
        );
        assert!(
            round_trip(&tuple)
                .span()
                .unwrap()
                .exact_eq(&Span::new(2, 9))
        );
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

    #[test]
    fn type_seat_entry_key_preserves_key_and_rejects_duplicates() {
        let seat = crate::TypeSeat {
            expr: crate::InterpExpr {
                kind: crate::InterpExprKind::Identifier("string".into()),
                span: tomet_span::Span::dummy(),
            },
            span: tomet_span::Span::dummy(),
        };
        let entry = TupleEntry::type_seat(seat.clone(), 42);
        assert_eq!(entry.name(), None);
        assert_eq!(entry.type_seat_key(), Some(&seat));
        assert!(matches!(entry.key(), Some(EntryKey::TypeSeat(_))));

        let tuple = Tuple::try_from_entries([entry.clone()]).unwrap();
        assert!(!tuple.has_no_named());
        assert!(tuple.has_no_unnamed());
        let keys: Vec<String> = tuple.keyed().map(|(k, _)| k.into_owned()).collect();
        assert_eq!(keys, ["<$string>"]);

        let dup = Tuple::try_from_entries([entry.clone(), entry]);
        assert!(dup.is_err());
    }
}
