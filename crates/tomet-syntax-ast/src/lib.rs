use serde::{Deserialize, Serialize};
use std::fmt;

pub mod cst_ast;
pub use cst_ast::*;

/// A 0-indexed byte offset and 1-indexed line/column position in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}

impl Position {
    pub fn new(line: usize, column: usize, offset: usize) -> Self {
        Self {
            line,
            column,
            offset,
        }
    }
}

/// A source span bounded by start and end [`Position`]s.
///
/// Note: [`PartialEq`] is implemented to always return `true` so that AST
/// structural equality checks (e.g. `assert_eq!(doc1, doc2)`) compare node
/// content and semantics without failing on source location differences.
/// Use [`exact_eq`](Self::exact_eq) or direct field comparison when exact
/// byte offsets need to be validated.
///
/// `Hash` is deliberately *not* derived: a derived impl would hash the real
/// `start`/`end` offsets while `PartialEq` above always returns `true`,
/// so two "equal" `Span`s could hash differently -- a Hash/Eq contract
/// violation that breaks `HashMap`/`HashSet` lookups. Don't re-add it
/// without also reconciling it with the custom equality above.
#[derive(Debug, Clone, Copy, Eq, Default, Serialize, Deserialize)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn dummy() -> Self {
        Self::default()
    }

    /// Returns `true` if this span represents a default/dummy span with zero positions.
    pub fn is_dummy(&self) -> bool {
        self.start == Position::default() && self.end == Position::default()
    }

    pub fn exact_eq(&self, other: &Self) -> bool {
        self.start == other.start && self.end == other.end
    }

    /// Combines two spans into a single span spanning from `self.start` to `other.end`.
    pub fn union(&self, other: &Self) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }

    /// Returns `true` if the given byte offset falls within this span (`start.offset..=end.offset`).
    pub fn contains_offset(&self, offset: usize) -> bool {
        self.start.offset <= offset && offset <= self.end.offset
    }
}

impl PartialEq for Span {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// A pure data value: the subset of Tomet with a direct serde
/// equivalent (no headings, prose, or links).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Seq(Vec<Value>),
    /// Insertion-ordered key/value pairs (a `.tmt` map has no inherent
    /// sort order, so preserve whatever order the source used).
    Map(Vec<(String, Value)>),
}

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Value::Null => serializer.serialize_none(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Int(i) => serializer.serialize_i64(*i),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::String(s) => serializer.serialize_str(s),
            Value::Seq(seq) => {
                use serde::ser::SerializeSeq;
                let mut s = serializer.serialize_seq(Some(seq.len()))?;
                for item in seq {
                    s.serialize_element(item)?;
                }
                s.end()
            }
            Value::Map(map) => {
                use serde::ser::SerializeMap;
                let mut m = serializer.serialize_map(Some(map.len()))?;
                for (k, v) in map {
                    m.serialize_entry(k, v)?;
                }
                m.end()
            }
        }
    }
}

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
                Ok(Value::Float(f64::from(v)))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Value::String(v.to_string()))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
                Ok(Value::String(v))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Null)
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Null)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(Value::Seq(items))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some((k, v)) = map.next_entry()? {
                    entries.push((k, v));
                }
                Ok(Value::Map(entries))
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub span: Span,
}

impl Document {
    pub fn new(blocks: Vec<Block>, span: Span) -> Self {
        Self { blocks, span }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Paragraph(Paragraph),
    /// A list is `Element { sigil: Sigil::Named("ol"|"ul"), value:
    /// Some(ElementValue::Children(items)), .. }`. `"ol"` vs `"ul"`
    /// distinguishes `-.` (auto-numbered) from plain `-` lists; numbering
    /// itself isn't stored, it's computed at render time. Each item is an
    /// `Element { sigil: Sigil::Bare, .. }` (legal only here, as an entry
    /// of `ElementValue::Children`); its `(...)` marker and trailing
    /// `{value}` attrs both use the ordinary `Value` grammar and are
    /// merged into that item `Element`'s `args`, and any nested sub-lists
    /// or indented blocks live in that item `Element`'s `children`.
    Element(Element),
}

impl Block {
    pub fn span(&self) -> Span {
        match self {
            Block::Paragraph(p) => p.span,
            Block::Element(e) => e.span,
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
}

impl Inline {
    pub fn span(&self) -> Span {
        match self {
            Inline::Text(t) => t.span,
            Inline::Element(e) => e.span,
        }
    }
}

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
/// own `@kind`, declared on its first line. A name from any other
/// namespace -- anything brought in with `@use` -- is always written
/// out, and a bare name in neither of the two is an error rather than
/// falling back to a `Custom` kind. That check is `Bindings::classify`
/// in `tomet-semantics`, not the free `classify_std` beside it, which only
/// knows `std`. This type only records the split.
///
/// Both halves are ASCII identifiers (`[A-Za-z_][A-Za-z0-9_-]*`). `.` is
/// the separator, so it is deliberately *not* an identifier character
/// here -- unlike in map keys, where `url.wiki` is one flat key.
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
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(ns) => write!(f, "{ns}.{}", self.name),
            None => f.write_str(&self.name),
        }
    }
}

/// Which sigil introduced an element, and its name.
///
/// There is one element sigil, `@`. It says "an element starts here" and
/// nothing else: whether the element stands as a block or belongs to a
/// paragraph is [`Placement`], which the parser derives from position, and
/// origin is carried by [`Name`]'s namespace.
///
/// Two earlier axes were tried here and both carried no information. `<T>`
/// vs `@name` was meant to separate official from user-defined elements,
/// but both classified through the same arm. `#name` vs `@name` was meant
/// to encode shape, but the parser never consulted it (position already
/// decided placement) and `tomet-semantics`' `required_shape` already knew
/// each builtin's shape, so the sigil only restated it. `#` is the heading
/// marker now, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sigil {
    /// `@name` -- an element. The name is mandatory.
    ///
    /// A nameless `@(url:...)` used to infer its kind from a key in its
    /// args. That inference was retired and `@link(target:...)` is the
    /// one link element, but the *syntax* was left behind, so a bare `@`
    /// still parsed and classified as the meaningless `Custom("at")`.
    /// It is gone: an element has a name.
    Named(Name),
    /// No sigil at all. Only legal as an entry inside another element's
    /// value group (e.g. the `(1)[...]` entries inside `@links{ ... }`),
    /// where the container already supplies the type.
    Bare,
    /// `${...}` interpolation -- structurally just a sigil with a
    /// mandatory `{value}` group, same shape as `@name{value}`, so it
    /// reuses `Element`/`Inline::Element` rather than being a separate
    /// `Inline` variant. No name of its own (unlike `Named`): the
    /// `InterpExpr` inside the `ElementValue::Interp` value group carries
    /// its own path/call name.
    Dollar,
}

impl Sigil {
    /// This element's name, if it has one.
    pub fn name(&self) -> Option<&Name> {
        match self {
            Sigil::Named(name) => Some(name),
            Sigil::Bare | Sigil::Dollar => None,
        }
    }

    /// An element with a bare (un-namespaced) name.
    pub fn named(name: impl Into<String>) -> Self {
        Sigil::Named(Name::bare(name))
    }

    /// Whether this element carries the bare (un-namespaced) name `name`.
    ///
    /// This is the check almost every consumer wants: a bare name resolves
    /// in `std` or in the document's own `@kind` namespace, never in one
    /// brought in with `@use`, so `is_bare_named("meta")` asks "is this
    /// *the* `meta` element" and cannot be satisfied by a user-defined
    /// `deck.meta`.
    ///
    /// It is a spelling test, not a resolution. A `@kind(writ)` document
    /// writes its own `@layers` bare as well; telling that apart from a
    /// `std` name is `Bindings::classify`'s job.
    pub fn is_bare_named(&self, name: &str) -> bool {
        self.name().is_some_and(|n| n.is_bare() && n.name == name)
    }
}

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

/// `(args)` / `[content]` / `{value}`, each optional and at most one of each,
/// in any order in the source.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Element {
    pub sigil: Sigil,
    pub placement: Placement,
    pub args: Option<Value>,
    pub content: Option<Vec<Inline>>,
    pub children: Option<Vec<Block>>,
    pub value: Option<ElementValue>,
    pub span: Span,
}

impl Default for Sigil {
    fn default() -> Self {
        Sigil::Bare
    }
}

/// One entry inside an element's `{value}` group: either a `key: value`
/// pair or a nested element.
///
/// `Pair`'s shape is exactly [`Value::Map`]'s entry type, which is what
/// lets the data view be rebuilt from a group without copying any other
/// structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    Pair(String, Value),
    Element(Element),
}

/// The contents of an element's value group.
///
/// `{...}` is always data: it parses uniformly into [`Entry`] items in
/// source order, whatever the element is called. "Is this a data map or a
/// list of children?" is no longer a parse-time branch -- it is a view
/// computed downstream in `tomet-semantics`, which is what lets the parser
/// build the tree without consulting any element vocabulary.
///
/// `Raw` is a `+++` fence body, captured verbatim. Whether it is later
/// read as JSON/YAML/TOML is decided after parsing, from the element's
/// `format:` arg -- the fence itself is opaque, so `format:` has no effect
/// on lexing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ElementValue {
    Group(Vec<Entry>),
    Raw(String),
    Interp(InterpExpr),
}

impl ElementValue {
    /// An empty group -- what `{}` parses to.
    pub fn empty_group() -> Self {
        ElementValue::Group(Vec::new())
    }

    /// Builds a group from `key: value` pairs alone.
    pub fn from_map(value: Value) -> Self {
        match value {
            Value::Map(entries) => ElementValue::Group(
                entries
                    .into_iter()
                    .map(|(k, v)| Entry::Pair(k, v))
                    .collect(),
            ),
            // A non-map body has no uniform-entry spelling; callers that
            // build one are constructing a single positional entry.
            other => ElementValue::Group(vec![Entry::Pair(String::new(), other)]),
        }
    }

    /// Builds a group from nested elements alone.
    pub fn from_children(children: Vec<Element>) -> Self {
        ElementValue::Group(children.into_iter().map(Entry::Element).collect())
    }

    /// The data view: this group's `key: value` pairs, in source order,
    /// as a [`Value::Map`]. Nested elements are skipped.
    ///
    /// Returns `None` for `Raw`/`Interp`, which carry no pairs.
    pub fn as_data(&self) -> Option<Value> {
        let ElementValue::Group(entries) = self else {
            return None;
        };
        Some(Value::Map(
            entries
                .iter()
                .filter_map(|e| match e {
                    Entry::Pair(k, v) => Some((k.clone(), v.clone())),
                    Entry::Element(_) => None,
                })
                .collect(),
        ))
    }

    /// The children view: this group's nested elements, in source order.
    pub fn as_children(&self) -> Vec<&Element> {
        let ElementValue::Group(entries) = self else {
            return Vec::new();
        };
        entries
            .iter()
            .filter_map(|e| match e {
                Entry::Element(el) => Some(el),
                Entry::Pair(..) => None,
            })
            .collect()
    }

    /// Whether this group holds any nested elements.
    pub fn has_children(&self) -> bool {
        matches!(self, ElementValue::Group(entries)
            if entries.iter().any(|e| matches!(e, Entry::Element(_))))
    }

    /// This group's `key: value` pairs, in source order.
    pub fn pairs(&self) -> impl Iterator<Item = (&String, &Value)> {
        let entries: &[Entry] = match self {
            ElementValue::Group(entries) => entries,
            _ => &[],
        };
        entries.iter().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k, v)),
            Entry::Element(_) => None,
        })
    }

    /// Mutable counterpart of [`ElementValue::pairs`].
    pub fn pairs_mut(&mut self) -> impl Iterator<Item = (&mut String, &mut Value)> {
        let entries: &mut [Entry] = match self {
            ElementValue::Group(entries) => entries,
            _ => &mut [],
        };
        entries.iter_mut().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k, v)),
            Entry::Element(_) => None,
        })
    }

    /// The value bound to `key`, if this group has such a pair.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.pairs().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// Mutable counterpart of [`ElementValue::get`].
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.pairs_mut().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    /// Appends a `key: value` pair. Does nothing for `Raw`/`Interp`, which
    /// hold no entries.
    pub fn push_pair(&mut self, key: impl Into<String>, value: Value) {
        if let ElementValue::Group(entries) = self {
            entries.push(Entry::Pair(key.into(), value));
        }
    }

    /// Removes the first pair bound to `key` and returns its value.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let ElementValue::Group(entries) = self else {
            return None;
        };
        let idx = entries
            .iter()
            .position(|e| matches!(e, Entry::Pair(k, _) if k == key))?;
        match entries.remove(idx) {
            Entry::Pair(_, v) => Some(v),
            Entry::Element(_) => unreachable!("position matched a Pair"),
        }
    }
}

/// One node of a `${...}` interpolation's parsed expression tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterpExpr {
    pub kind: InterpExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum InterpExprKind {
    Identifier(String),
    Literal(Literal),
    Call {
        callee: Box<InterpExpr>,
        args: Vec<InterpExpr>,
    },
    Member {
        object: Box<InterpExpr>,
        member: String,
    },
    NamedArg {
        name: String,
        value: Box<InterpExpr>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_and_span() {
        let p1 = Position::new(1, 1, 0);
        let p2 = Position::new(1, 10, 9);
        let span = Span::new(p1, p2);

        assert!(!span.is_dummy());
        assert!(span.contains_offset(0));
        assert!(span.contains_offset(5));
        assert!(span.contains_offset(9));
        assert!(!span.contains_offset(10));

        let dummy = Span::dummy();
        assert!(dummy.is_dummy());

        assert_eq!(span, dummy);
        assert!(!span.exact_eq(&dummy));
    }
}
