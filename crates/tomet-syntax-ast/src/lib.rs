//! =[ tomet-ast ]
//!
//! Shared AST and `Value` data models for the Tomet toolchain ([`File`], [`Block`],
//! [`Inline`], [`Element`], [`ElementView`], [`BodyView`], [`Declaration`], [`Interpolation`],
//! [`Hole`], [`ListItem`], [`Value`], [`Span`]).
//!
//! =[ Design Policy ]
//!
//! - (Pure Data Models & Structural Accessors Only)
//!   | `tomet-ast` is the foundational AST crate referenced across the workspace,
//!   | depending only on `tomet-span`, `serde`, and `unicode-width`.
//!   | It contains only struct/enum definitions and pure inherent methods for construction
//!   | (`new`, `with_*`) and structural inspection (`as_str()`, `get()`, `is_null()`).
//! - (A Node's Role Is Its Type)
//!   | A `${...}` is an [`Interpolation`] (an expression and a span), not an [`Element`]:
//!   | it has no name, no groups and no connects, so it is a variant of [`Block`],
//!   | [`Inline`] and [`Value`] and no walker over elements ever meets one. Reach every
//!   | interpolation of a file through `tomet_tree::for_each_interpolation`.
//!   | A `::name(...)` is a [`Declaration`]: sits in the file preamble header ([`File::header`]),
//!   | about the file and attached to nothing, so it has no placement and takes no connect.
//!   | A `:name(...)` is a [`Connect`]: metadata about its owner, with no id, no connects of its own
//!   | and no placement. A `<?matcher(...)>` is a [`Hole`] (a matcher, optional, in place of a name).
//!   | `<...>` is one family in the *syntax* (the CST has one `SLOT` node, whatever the mark inside); the
//!   | AST has no node for the family. It names nodes by role, never by spelling, and has a node only
//!   | for what tomet interprets or what carries structure to walk: a hole is a [`Hole`], a name tag
//!   | (`<PATH>`) is text. Merging nodes of different structure into one type with a field to tell
//!   | them apart is what split `Element` before; a new kind of seat is judged by that rule.
//!   | A list item is a [`ListItem`] and a nameless `(marker)[content]` entry of a `{...}` group is a
//!   | [`Bare`]: neither has a name, so neither is an [`Element`], whose name is mandatory.
//!   | [`ListItem::as_element_view`] gives a zero-copy [`ElementView`] of the element a `-@name(...)`
//!   | item is.
//!   | What nodes share is [`Data`] (`(args)` and the `{...}` [`Group`]), and their body groups can
//!   | be inspected uniformly via [`BodyView`]; `[content]` and `#(id)` stay on the roles that take them.
//! - (Clear Crate Boundaries)
//!   | Semantics: Classification of element meanings (links, `::meta`, `::config`) lives in `tomet-semantics`.
//!   | Traversal & Manipulation: Generic tree traversal algorithms live in `tomet-tree`.
//!   | Formatting & Printing: Serialization to `.tmt` text lives in `tomet-style` and `tomet-printer`.
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
    Null,
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
    Seq(Vec<Value>),
    /// Insertion-ordered key/value pairs (a `.tmt` map has no inherent
    /// sort order, so preserve whatever order the source used).
    Map(Vec<(String, Value)>),
    /// `name(arg, arg, ...)` -- a call the parser did not resolve.
    /// Positional-only: a call's arguments are never `key: value` pairs, an
    /// ordered sequence is enough for what this exists to express.
    ///
    /// The parser resolves exactly the literal constructors
    /// (`list`, `string`, `int`, `float`) to
    /// the plain value they stand for, so `list(card)` is a [`Value::Seq`]
    /// and never a `Call`. Every other callee -- `enum`, a typo -- is kept
    /// here as an uninterpreted `String`, the same way the parser never
    /// judges a `:name(...)` connect's name; only `tomet-semantics` knows
    /// which of those are real.
    ///
    /// Deliberately not the same mechanism as `${func(args)}`'s
    /// `InterpExprKind::Call`: that callee can itself be a path
    /// expression and is evaluated later, while this is a literal fixed
    /// at parse time. Sharing surface syntax (`name(args)`) is coincidence,
    /// not kinship -- conflating the two would blur which one is
    /// evaluated for a reader of either.
    Call(String, Vec<Value>),
    /// `${...}` / `$name(...)` sitting where a value goes -- evaluated later,
    /// so `tomet-semantics` never looks inside it as data. See [`Interpolation`].
    Interp(Interpolation),
    /// A `<?matcher(...)>` hole sitting where a value goes. See [`Hole`].
    Hole(Box<Hole>),
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
    /// lets the same thing be a named `key: @name(...)` value, and a
    /// `[...]` sequence item, too, since `Value` is the type all three
    /// positions share.
    ///
    /// Parsed with `allow_colon_connect: false`, same as
    /// [`crate::Entry::Element`] -- `:name(...)` stays unavailable inside
    /// any value position, deliberately: a `{...}`/`(...)` group is data,
    /// and a connect changes what the *enclosing* element means, which
    /// has no sense for an element that is itself sitting inside a value.
    Element(Box<Element>),
    /// `key: [ ... ]` -- block content (the same `Vec<Block>` grammar as
    /// an element's own `[content]`/`|content` slot, see
    /// `tmtroot/docs/spec/feature/content-shape.tmt`) sitting as a named
    /// value inside `(args)`/`{data}`, e.g. `@conflict(a: [ ... ], b: [
    /// ... ])`. `[a, b]` array literals were retired in favor of
    /// `list(...)` specifically to free `[...]` for this -- it already
    /// meant "block content" at the element level, and now means it
    /// uniformly at any value position.
    Blocks(Vec<Block>),
}

impl Value {
    /// How an `f64` is written when it is a [`Value::Float`]: the shortest text
    /// that reads back as the same number, always with a `.` or an exponent.
    ///
    /// This is the one definition. The reader keeps a number as a `Float` only
    /// when the text it was written as is this text, and the printer writes a
    /// `Float` with it, so a `Float` is always what its text says.
    pub fn float_text(f: f64) -> String {
        format!("{f:?}")
    }

    /// The text a bare scalar was written as, for a target that wants text: a
    /// `string` argument given `2024`, `true` or `1.10`.
    ///
    /// `None` for what is not a scalar (`null`, a list, a map, an element) --
    /// those have no single text. A [`String`](Value::String) is itself,
    /// whether it was quoted or not: the reader does not keep the difference.
    pub fn scalar_text(&self) -> Option<std::borrow::Cow<'_, str>> {
        use std::borrow::Cow;
        Some(match self {
            Value::String(s) | Value::Number(s) => Cow::Borrowed(s.as_str()),
            Value::Int(i) => Cow::Owned(i.to_string()),
            Value::Float(f) => Cow::Owned(Value::float_text(*f)),
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
            // A number on the wire is an f64: that is the most a serializer
            // that has no arbitrary-precision number can say. One that cannot
            // be one (too large) is sent as the text it was written as.
            Value::Number(text) => match text.parse::<f64>() {
                Ok(f) if f.is_finite() => serializer.serialize_f64(f),
                _ => serializer.serialize_str(text),
            },
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
            Value::Call(name, args) => {
                use serde::ser::SerializeMap;
                let mut m = serializer.serialize_map(Some(2))?;
                m.serialize_entry("call", name)?;
                m.serialize_entry("args", args)?;
                m.end()
            }
            Value::Interp(interp) => interp.serialize(serializer),
            Value::Hole(hole) => hole.serialize(serializer),
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

// Deliberately no `Value::Call`, `Value::Interp`, `Value::Hole`,
// `Value::Element`, or `Value::Blocks` arm below: all five are only ever produced by
// `tomet-syntax-parser` reading source text (`name(...)` / `${...}` /
// `@name(...)` / `key: [...]`), never by
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
    /// A `${...}` that stands alone on its line, in block context. It is a
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
            Block::Interp(i) => i.span,
        }
    }
}

/// A hierarchical section: introduced by `=`, `==`, etc.
///
/// Contains its nesting level (`=` is 1, `==` is 2, etc.), title inlines,
/// optional arguments (`(id: intro)`), optional value group (`{attrs}`),
/// connects (`:as(...)`), and child blocks (paragraphs, lists, sub-sections)
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
/// what a `:rule(allow:list(...))` attached to the list itself (restricting
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
/// `{value}` its attributes, `#(id)` and `:connect`s -- and its `content` is
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
    /// `${...}` / `$name(...)` inside running text. See [`Interpolation`].
    Interp(Interpolation),
    /// A `<?matcher(...)>` hole inside running text. See [`Hole`].
    Hole(Hole),
    /// A source line break that is semantically whitespace. Unlike the old
    /// behavior of folding it into a literal `' '` (or nothing, between two
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
    /// and `Raw` do, `Element`/`Interp`/`SoftBreak`/`LineBreak` don't.
    ///
    /// Used by renderers reproducing [`softbreak_join`]'s wide-character
    /// check against whatever inline follows a `SoftBreak`: only the
    /// immediate next item is consulted, deliberately not skipping past an
    /// `Element` to find text beyond it, matching the fold's old behavior of
    /// only ever seeing characters within its own text run.
    pub fn first_char(&self) -> Option<char> {
        match self {
            Inline::Text(t) => t.value.chars().next(),
            Inline::Raw(t) => t.value.chars().next(),
            Inline::Element(_)
            | Inline::Interp(_)
            | Inline::Hole(_)
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
/// here, shared, because it used to be baked into the parser and is now
/// needed by several independent call sites instead: HTML, Typst, and
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
/// `(args)[content]{value}` groups and before any `:name(...)` connect.
///
/// A plain `String`, not [`Value`]: `#(...)` is parsed with a narrower,
/// scalar-only grammar (a bare identifier-like token or a quoted
/// string) than the general value grammar `(args)`/`{value}` use, so a
/// map/seq/call can never end up here -- unlike `Name`, there is no
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
/// [`Element`] it takes no `:connect`.
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

/// A nameless entry of a `{...}` group, `(marker)[content]`: the container
/// already supplies what it is (the `(1)[...]` entries of `@links(){ ... }`),
/// so it has no name. It takes the groups an element does and no connect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bare {
    pub data: Data,
    pub content: Option<Vec<Block>>,
    pub id: Option<Id>,
    pub span: Span,
}

impl Bare {
    /// A zero-copy borrowed view of this bare entry's body groups.
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
/// optional (`<?>` and `<?(n)>` have none); and it takes no `:connect`. A box that does not close
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

/// A `:name(...)` connect stacked after an element, a section or a list
/// (`@x(...):as(y):rule(...)`): metadata about its owner, not content of the
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

/// The data a node carries: its `(args)` and its `{value}` group, each
/// optional and at most one of each.
///
/// Every node that takes either holds them as a `Data`, so what reads them
/// (the walkers, the printer, `merge_values`) is written once for all of
/// them. `[content]` and `#(id)` are not in here on purpose: they differ in
/// kind (child blocks, an address) and not every node takes them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Data {
    pub args: Option<Value>,
    pub value: Option<Group>,
}

/// An `@name(args)[content]{value}` element.
///
/// The one element sigil is `@`; it says "an element starts here" and nothing
/// else. Whether the element stands as a block or belongs to a paragraph is
/// [`Placement`], which the parser derives from position, and origin is
/// carried by [`Name`]'s namespace. Two earlier axes were tried and carried no
/// information: `<T>` vs `@name` (official vs user-defined, but both
/// classified through the same arm) and `#name` vs `@name` (shape, which
/// `tomet-semantics`' `required_shape` already knew). `#(id)` is not a second
/// sigil either: it is [`Element::id`], a slot in the element's own groups.
///
/// A node that has no name of its own, a [`Bare`] entry or a plain list item,
/// is not an element at all.
///
/// `(args)` / `[content]` / `{value}`, each optional and at most one of each,
/// in any order in the source.
///
/// `id` is a single slot, written `#(foobar)`, read after those three
/// groups but strictly before any connect -- see [`Id`].
///
/// `connects` is unrelated to those three: it is the `:name(...)` family
/// stacked after them (`@x(...):as(y):rule(...)`), zero or more, each a
/// [`Connect`]. Order-preserving, empty when nothing follows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    /// The name after `@`. It is mandatory: a nameless `@(url:...)` used to
    /// infer its kind from a key in its args, that inference was retired,
    /// and the syntax is gone with it.
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
    pub fn args(&self) -> Option<&'a Value> {
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
/// [`Bare`], [`Connect`], [`Hole`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyView<'a> {
    pub data: &'a Data,
    pub content: Option<&'a [Block]>,
    pub id: Option<&'a Id>,
}

impl<'a> BodyView<'a> {
    /// Positional or keyword arguments `(args)`.
    pub fn args(&self) -> Option<&'a Value> {
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
/// `Pair`'s shape is exactly [`Value::Map`]'s entry type, which is what
/// lets the data view be rebuilt from a group without copying any other
/// structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    Pair(String, Value),
    /// A named entry, `@name(...)`.
    Element(Element),
    /// A nameless entry, `(marker)[content]`. See [`Bare`].
    Bare(Bare),
    /// A `<?matcher(...)>` hole standing as an entry. See [`Hole`].
    Hole(Hole),
}

/// The entries of a `{...}` group, in source order.
///
/// `{...}` is always data: it parses uniformly into [`Entry`] items in
/// source order, whatever the element is called. "Is this a data map or a
/// list of children?" is no longer a parse-time branch -- it is a view
/// computed downstream in `tomet-semantics`, which is what lets the parser
/// build the tree without consulting any element vocabulary.
///
/// `Group` holds a source-order sequence of entries (`Group(pub Vec<Entry>)`)
/// and provides map operations exclusively for [`Entry::Pair`] entries.
/// Non-pair entries ([`Entry::Element`], [`Entry::Bare`], [`Entry::Hole`]) are
/// handled directly by callers through exhaustive pattern matching.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Group(pub Vec<Entry>);

impl Group {
    /// Builds a group from `key: value` pairs alone.
    pub fn from_map(value: Value) -> Self {
        match value {
            Value::Map(entries) => Group(
                entries
                    .into_iter()
                    .map(|(k, v)| Entry::Pair(k, v))
                    .collect(),
            ),
            // A non-map body has no uniform-entry spelling; callers that
            // build one are constructing a single positional entry.
            other => Group(vec![Entry::Pair(String::new(), other)]),
        }
    }

    /// The data view: this group's `key: value` pairs, in source order,
    /// as a [`Value::Map`]. Non-pair entries are skipped.
    pub fn as_data(&self) -> Value {
        let Group(entries) = self;
        Value::Map(
            entries
                .iter()
                .filter_map(|e| match e {
                    Entry::Pair(k, v) => Some((k.clone(), v.clone())),
                    Entry::Element(_) | Entry::Bare(_) | Entry::Hole(_) => None,
                })
                .collect(),
        )
    }

    /// This group's `key: value` pairs, in source order.
    pub fn pairs(&self) -> impl Iterator<Item = (&String, &Value)> {
        let Group(entries) = self;
        entries.iter().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k, v)),
            Entry::Element(_) | Entry::Bare(_) | Entry::Hole(_) => None,
        })
    }

    /// Mutable counterpart of [`Group::pairs`].
    pub fn pairs_mut(&mut self) -> impl Iterator<Item = (&mut String, &mut Value)> {
        let Group(entries) = self;
        entries.iter_mut().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k, v)),
            Entry::Element(_) | Entry::Bare(_) | Entry::Hole(_) => None,
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
            Entry::Element(_) | Entry::Bare(_) | Entry::Hole(_) => {
                unreachable!("position matched a Pair")
            }
        }
    }
}

/// A `${...}` (or `$name(...)`) interpolation: an expression to be evaluated
/// where it stands.
///
/// It is not an [`Element`]: it has no name, no `(args)` / `[content]` and no
/// `#(id)`, and it never takes a `:connect`. It holds an expression and the
/// span of the whole `$`-form, and nothing else. It sits in three places, as
/// [`Block::Interp`], [`Inline::Interp`] and [`Value::Interp`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interpolation {
    pub expr: InterpExpr,
    pub span: Span,
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
    Id(String),
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
    /// `lhs op rhs`. Comparison and `&&` / `||` only; see
    /// `tmtroot/spec/expressions/operators.expr.tmt`.
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
    /// `x => body` or `(a, b) => body`. The body is one expression.
    Lambda {
        params: Vec<String>,
        body: Box<InterpExpr>,
    },
}

/// A binary operator in an expression.
///
/// There is no arithmetic and no `!`: both are deliberately functions
/// (`add`, `not`), because `-` already means a list marker and part of an
/// identifier, and the expression language keeps symbols to the minimum.
/// `&&` and `||` are the only operators that may skip their right-hand
/// side, which is why they are symbols and not functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
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
    /// The spelling in source.
    pub fn symbol(self) -> &'static str {
        match self {
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

    /// Whether this is one of the six comparisons, which do not chain.
    pub fn is_comparison(self) -> bool {
        !matches!(self, BinaryOp::And | BinaryOp::Or)
    }
}

/// The source spelling of an interpolation expression -- the interior of
/// `${...}`, without the surrounding `${` and `}`. The caller adds those,
/// because `$name(args)` writes the same expression with no braces at all.
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
            InterpExprKind::Literal(Literal::Int(i)) => write!(f, "{i}"),
            InterpExprKind::Literal(Literal::Float(x)) => write!(f, "{x}"),
            InterpExprKind::Literal(Literal::String(s)) => write_quoted(f, s),
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
            InterpExprKind::Literal(Literal::Bool(b)) => write!(f, "{b}"),
            InterpExprKind::Literal(Literal::Null) => f.write_str("null"),
            InterpExprKind::Member { object, member } => write!(f, "{object}.{member}"),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
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

    #[test]
    fn test_element_view_and_list_item_as_element_view() {
        let name = Name::bare("item");
        let data = Data {
            args: Some(Value::Int(42)),
            value: Some(Group::from_map(Value::Map(vec![(
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
        assert_eq!(el_view.args(), Some(&Value::Int(42)));
        assert!(el_view.value().is_some());
        assert_eq!(el_view.id, Some(&id));
        assert_eq!(el_view.connects, &connects);

        // Body view matches
        let body = el_view.body();
        assert_eq!(body, item.as_body_view());
        assert_eq!(body, element.as_body_view());
        assert_eq!(body.args(), Some(&Value::Int(42)));
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

        // BodyView on Declaration, Bare, Hole, Connect
        let decl = Declaration {
            name: name.clone(),
            data: data.clone(),
            content: Some(content_blocks.clone()),
            id: Some(id.clone()),
            span: Span::dummy(),
        };
        assert_eq!(decl.as_body_view(), body);

        let bare = Bare {
            data: data.clone(),
            content: Some(content_blocks.clone()),
            id: Some(id.clone()),
            span: Span::dummy(),
        };
        assert_eq!(bare.as_body_view(), body);

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
}
