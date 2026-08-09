//! AST types shared by `typedmark-parser` and its consumers.
//!
//! `Value` is the pure-data subset (maps onto serde's data model 1:1) --
//! it's what fills a `(input)` or `{value}` group, and it's also the whole
//! result of parsing a data-only `.tm` file (see `typedmark-serde`).
//!
//! `Document`/`Block`/`Inline`/`Element` are the full markup AST: headings,
//! paragraphs, lists, and typed elements, in source order.

/// A pure data value: the subset of TypedMark with a direct serde
/// equivalent (no headings, prose, or links).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Seq(Vec<Value>),
    /// Insertion-ordered key/value pairs (a `.tm` map has no inherent
    /// sort order, so preserve whatever order the source used).
    Map(Vec<(String, Value)>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(Heading),
    Paragraph(Vec<Inline>),
    /// `ordered` distinguishes `-.` (auto-numbered) from plain `-` lists;
    /// numbering itself isn't stored, it's computed at render time.
    List {
        ordered: bool,
        items: Vec<ListItem>,
    },
    Element(Element),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    /// Number of leading `#` characters.
    pub level: u8,
    pub content: Vec<Inline>,
    pub attrs: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub content: Vec<Inline>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Element(Element),
}

/// Which sigil introduced a typed element, and its name if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sigil {
    /// `<name>` -- name is mandatory.
    Type(String),
    /// `@name` or bare `@` -- name is optional; when absent, the element's
    /// kind is inferred from a key inside its `input` group (e.g.
    /// `@(url:...)` is a link because `url` is a recognized link key).
    At(Option<String>),
    /// No sigil at all. Only legal as an entry inside another element's
    /// `ElementValue::Children` (e.g. the `(1)[...]` entries inside
    /// `@links{ ... }`), where the container already supplies the type.
    Bare,
}

/// `(input)` / `[area]` / `{value}`, each optional and at most one of each,
/// in any order in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub sigil: Sigil,
    pub input: Option<Value>,
    pub area: Option<Vec<Inline>>,
    pub value: Option<ElementValue>,
}

impl Element {
    pub fn new(sigil: Sigil) -> Self {
        Element {
            sigil,
            input: None,
            area: None,
            value: None,
        }
    }
}

/// The contents of an element's `{value}` group: either plain data, or (for
/// container elements like `@links{}`) a list of nested elements.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementValue {
    Data(Value),
    Children(Vec<Element>),
}
