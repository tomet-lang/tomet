/// A 0-indexed byte offset and 1-indexed line/column position in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
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
#[derive(Debug, Clone, Copy, Eq, Default, Hash)]
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

    pub fn exact_eq(&self, other: &Self) -> bool {
        self.start == other.start && self.end == other.end
    }
}

impl PartialEq for Span {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

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
    pub span: Span,
}

impl Document {
    pub fn new(blocks: Vec<Block>, span: Span) -> Self {
        Self { blocks, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(Heading),
    Paragraph(Paragraph),
    /// `ordered` distinguishes `-.` (auto-numbered) from plain `-` lists;
    /// numbering itself isn't stored, it's computed at render time.
    List(List),
    Element(Element),
}

impl Block {
    pub fn span(&self) -> Span {
        match self {
            Block::Heading(h) => h.span,
            Block::Paragraph(p) => p.span,
            Block::List(l) => l.span,
            Block::Element(e) => e.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Paragraph {
    pub content: Vec<Inline>,
    pub span: Span,
}

impl Paragraph {
    pub fn new(content: Vec<Inline>, span: Span) -> Self {
        Self { content, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct List {
    pub ordered: bool,
    pub items: Vec<ListItem>,
    pub span: Span,
}

impl List {
    pub fn new(ordered: bool, items: Vec<ListItem>, span: Span) -> Self {
        Self {
            ordered,
            items,
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    /// Number of leading `#` characters.
    pub level: u8,
    pub content: Vec<Inline>,
    pub attrs: Option<Value>,
    pub span: Span,
}

impl Heading {
    pub fn new(level: u8, content: Vec<Inline>, attrs: Option<Value>, span: Span) -> Self {
        Self {
            level,
            content,
            attrs,
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub content: Vec<Inline>,
    /// Optional status marker inside `[...]` or `(...)` (e.g. `" "` for `( )`, `"x"` for `(x)`, `"T"` for `(T)`, `"?"` for `(?)`).
    /// `None` for plain items without bracket/paren status markers.
    pub marker: Option<String>,
    /// Optional attributes attached via trailing `{value}` group (e.g. `{tag: dev}`).
    pub attrs: Option<Value>,
    pub span: Span,
}

impl ListItem {
    pub fn new(
        content: Vec<Inline>,
        marker: Option<String>,
        attrs: Option<Value>,
        span: Span,
    ) -> Self {
        Self {
            content,
            marker,
            attrs,
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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

/// Which sigil introduced a typed element, and its name if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sigil {
    /// `<name>` -- name is mandatory.
    Type(String),
    /// `@name` or bare `@` -- name is optional; when absent, the element's
    /// kind is inferred from a key inside its `args` group via
    /// [`infer_at_kind`] (e.g. `@(url:...)` is a link because `url` is a
    /// recognized link key).
    At(Option<String>),
    /// No sigil at all. Only legal as an entry inside another element's
    /// `ElementValue::Children` (e.g. the `(1)[...]` entries inside
    /// `@links{ ... }`), where the container already supplies the type.
    Bare,
}

/// Keys recognized in a bare `@(key:...)` element's `args` map, checked
/// in this order, to infer its kind when no explicit `@name` is given.
///
/// This exists only for `url`/`file`/`ref`, which need to work *inline* in
/// running prose where every character counts -- it's not a general
/// "authors may omit the name" convenience. `meta` is deliberately **not**
/// here: `@meta(format:tag){...}` is always block-level, never needs to be
/// terse, so it always takes the explicit name. (A bare `@(meta:yaml){...}`
/// would otherwise look like it means the same thing as
/// `@meta(format:yaml){...}` but silently not be: real JSON/YAML/TOML
/// parsing is driven by a `format` key in `args`, checked independently of
/// the sigil/name -- see `typedmark_parser`'s `embedded_format` module --
/// and a bare `@(meta:yaml)`'s `args` has no such key.) `links` is
/// explicit-name-only for the same reason `meta` now is -- it was never in
/// this list.
pub const INFERRED_AT_KEYS: [&str; 3] = ["url", "file", "ref"];

/// Infers a bare `@(...)` element's kind: whichever of [`INFERRED_AT_KEYS`]
/// appears first as a top-level key in `args`. `None` if `args` isn't a
/// map, or none of those keys are present (the caller then typically falls
/// back to a generic "at" kind).
pub fn infer_at_kind(args: Option<&Value>) -> Option<&'static str> {
    let Some(Value::Map(entries)) = args else {
        return None;
    };
    INFERRED_AT_KEYS
        .iter()
        .find(|key| entries.iter().any(|(k, _)| k == *key))
        .copied()
}

/// `(args)` / `[content]` / `{value}`, each optional and at most one of each,
/// in any order in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub sigil: Sigil,
    pub args: Option<Value>,
    pub content: Option<Vec<Inline>>,
    pub value: Option<ElementValue>,
    pub span: Span,
}

impl Element {
    pub fn new(sigil: Sigil) -> Self {
        Element {
            sigil,
            args: None,
            content: None,
            value: None,
            span: Span::default(),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }
}

/// The contents of an element's `{value}` group: either plain data, or (for
/// container elements like `@links{}`) a list of nested elements.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementValue {
    Data(Value),
    Children(Vec<Element>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_url_file_ref_from_a_bare_at_element() {
        for key in ["url", "file", "ref"] {
            let args = Value::Map(vec![(key.to_string(), Value::String("x".into()))]);
            assert_eq!(infer_at_kind(Some(&args)), Some(key));
        }
    }

    #[test]
    fn does_not_infer_meta_from_a_bare_at_element() {
        let args = Value::Map(vec![("meta".to_string(), Value::String("yaml".into()))]);
        assert_eq!(infer_at_kind(Some(&args)), None);
    }

    #[test]
    fn no_recognized_key_or_non_map_args_infers_nothing() {
        assert_eq!(infer_at_kind(None), None);
        assert_eq!(infer_at_kind(Some(&Value::String("x".into()))), None);
        let args = Value::Map(vec![("other".to_string(), Value::Int(1))]);
        assert_eq!(infer_at_kind(Some(&args)), None);
    }
}
