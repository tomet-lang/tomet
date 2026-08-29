pub mod cst_ast;
pub use cst_ast::*;

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
///
/// `Hash` is deliberately *not* derived: a derived impl would hash the real
/// `start`/`end` offsets while `PartialEq` above always returns `true`,
/// so two "equal" `Span`s could hash differently -- a Hash/Eq contract
/// violation that breaks `HashMap`/`HashSet` lookups. Don't re-add it
/// without also reconciling it with the custom equality above.
#[derive(Debug, Clone, Copy, Eq, Default)]
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

impl Value {
    /// Returns `true` if this value is `Value::Null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Returns the string slice if this value is a `Value::String`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns the `bool` value if this value is a `Value::Bool`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns the `i64` value if this value is a `Value::Int`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the `f64` value if this value is a `Value::Float`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns a slice of elements if this value is a `Value::Seq`.
    pub fn as_seq(&self) -> Option<&[Value]> {
        match self {
            Value::Seq(seq) => Some(seq.as_slice()),
            _ => None,
        }
    }

    /// Returns a slice of key-value pairs if this value is a `Value::Map`.
    pub fn as_map(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Map(map) => Some(map.as_slice()),
            _ => None,
        }
    }

    /// If this value is a `Value::Map`, returns the first value associated with `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(entries) => entries
                .iter()
                .find_map(|(k, v)| if k == key { Some(v) } else { None }),
            _ => None,
        }
    }

    /// If this value is a `Value::Map`, returns a mutable reference to the first value associated with `key`.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self {
            Value::Map(entries) => entries
                .iter_mut()
                .find_map(|(k, v)| if k == key { Some(v) } else { None }),
            _ => None,
        }
    }

    /// Returns `true` if a sequence or map is empty, or if string is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Seq(seq) => seq.is_empty(),
            Value::Map(map) => map.is_empty(),
            _ => false,
        }
    }
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
    Paragraph(Paragraph),
    /// A list is `Element { sigil: Sigil::Type("ol"|"ul"), value:
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
    /// `tomet_semantics::infer_at_kind` (e.g. `@(url:...)` is a link
    /// because `url` is a recognized link key).
    At(Option<String>),
    /// No sigil at all. Only legal as an entry inside another element's
    /// `ElementValue::Children` (e.g. the `(1)[...]` entries inside
    /// `@links{ ... }`), where the container already supplies the type.
    Bare,
    /// `${...}` interpolation -- structurally just a sigil with a
    /// mandatory `{value}` group, same shape as `@name{value}`, so it
    /// reuses `Element`/`Inline::Element` rather than being a separate
    /// `Inline` variant. No name of its own (unlike `Type`/`At`): the
    /// `InterpExpr` inside the `ElementValue::Interp` value group carries
    /// its own path/call name.
    Dollar,
}

/// `(args)` / `[content]` / `{value}`, each optional and at most one of each,
/// in any order in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub sigil: Sigil,
    pub args: Option<Value>,
    pub content: Option<Vec<Inline>>,
    pub children: Option<Vec<Block>>,
    pub value: Option<ElementValue>,
    pub span: Span,
}

impl Element {
    pub fn new(sigil: Sigil) -> Self {
        Element {
            sigil,
            args: None,
            content: None,
            children: None,
            value: None,
            span: Span::default(),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    pub fn with_args(mut self, args: Value) -> Self {
        self.args = Some(args);
        self
    }

    pub fn with_content(mut self, content: Vec<Inline>) -> Self {
        self.content = Some(content);
        self
    }

    pub fn with_children(mut self, children: Vec<Block>) -> Self {
        self.children = Some(children);
        self
    }

    pub fn with_value(mut self, value: ElementValue) -> Self {
        self.value = Some(value);
        self
    }

    /// Returns the element's explicit name if introduced via `Sigil::Type("name")` or `Sigil::At(Some("name"))`.
    pub fn name(&self) -> Option<&str> {
        match &self.sigil {
            Sigil::Type(name) => Some(name.as_str()),
            Sigil::At(Some(name)) => Some(name.as_str()),
            Sigil::At(None) | Sigil::Bare | Sigil::Dollar => None,
        }
    }

    /// Returns `true` if this element has `Sigil::Bare`.
    pub fn is_bare(&self) -> bool {
        matches!(self.sigil, Sigil::Bare)
    }

    /// A list: `sigil` is `Type("ol")` if `ordered`, else `Type("ul")`;
    /// `items` become `ElementValue::Children`. `ordered` distinguishes
    /// `-.` (auto-numbered) from plain `-` lists -- numbering itself isn't
    /// stored, it's computed at render time.
    pub fn list(ordered: bool, items: Vec<Element>, span: Span) -> Self {
        Element {
            sigil: Sigil::Type(if ordered { "ol" } else { "ul" }.to_string()),
            args: None,
            content: None,
            children: None,
            value: Some(ElementValue::Children(items)),
            span,
        }
    }

    /// A list item: legal only as an entry of a [`Element::list`]'s
    /// `Children`. `marker` is the optional `(...)`-shaped marker (goes to
    /// `args`, parsed with the same `Value` grammar as any other
    /// element's `(args)`, normalized against the builtin `"marker"`
    /// positional key -- see `tomet-semantics::positional`); `attrs`
    /// is the optional trailing `{value}`-shaped attributes (e.g.
    /// `{tag: dev}`, goes to `value` as `ElementValue::Data`); `children`
    /// is any nested sub-lists or indented blocks.
    pub fn list_item(
        content: Vec<Inline>,
        marker: Option<Value>,
        attrs: Option<Value>,
        children: Vec<Block>,
        span: Span,
    ) -> Self {
        Element {
            sigil: Sigil::Bare,
            args: marker,
            content: Some(content),
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
            value: attrs.map(ElementValue::Data),
            span,
        }
    }
}

/// The contents of an element's `{value}` group: either plain data, or (for
/// container elements like `@links{}`) a list of nested elements, or (for
/// `Sigil::Dollar`'s `${...}` only) an unresolved interpolation expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementValue {
    Data(Value),
    Children(Vec<Element>),
    Interp(InterpExpr),
}

/// One node of a `${...}` interpolation's parsed expression tree.
/// Grammar-only: this is an unresolved syntax tree -- looking up an
/// `Identifier`/`Member`'s referenced element or calling a `Call`'s
/// function is `tomet-resolver`/`tomet-compute`'s job, not this
/// crate's. Deliberately not `Value`-shaped: `Value` can't distinguish a
/// bare identifier reference from a quoted string literal (both collapse
/// to the same `Value::String` once parsed), and can't represent a
/// nested `Call`/`Member` as an argument or object.
///
/// No infix operators yet (`${a + b}` stays an open idea, not parsed).
#[derive(Debug, Clone, PartialEq)]
pub struct InterpExpr {
    pub kind: InterpExprKind,
    pub span: Span,
}

/// `Call`'s `callee` and `Member`'s `object` are boxed sub-expressions
/// (not a bare `String` name), so postfix chains compose freely:
/// `a.b(x)` (call a member) and `b(x).id` (access a member of a call's
/// result) are both just nested `Member`/`Call` wrapping, not two
/// separate mechanisms. A plain dotted path like `a.b.c` is the same
/// `Member` recursion with no `Call` in the chain -- there's no separate
/// flat `Path` variant, since `Member` alone already covers it.
#[derive(Debug, Clone, PartialEq)]
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
}

#[derive(Debug, Clone, PartialEq)]
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

        // PartialEq on Span always returns true for test comparison convenience
        assert_eq!(span, dummy);
        // exact_eq checks field equality
        assert!(!span.exact_eq(&dummy));

        let p3 = Position::new(2, 5, 20);
        let span2 = Span::new(p2, p3);
        let union_span = span.union(&span2);
        assert_eq!(union_span.start, p1);
        assert_eq!(union_span.end, p3);
    }

    #[test]
    fn test_value_helpers() {
        let null_val = Value::Null;
        assert!(null_val.is_null());
        assert!(null_val.is_empty());

        let str_val = Value::String("hello".to_string());
        assert_eq!(str_val.as_str(), Some("hello"));
        assert!(!str_val.is_empty());

        let bool_val = Value::Bool(true);
        assert_eq!(bool_val.as_bool(), Some(true));

        let int_val = Value::Int(42);
        assert_eq!(int_val.as_i64(), Some(42));

        let float_val = Value::Float(3.14);
        assert_eq!(float_val.as_f64(), Some(3.14));

        let seq_val = Value::Seq(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(seq_val.as_seq().map(|s| s.len()), Some(2));
        assert!(!seq_val.is_empty());

        let mut map_val = Value::Map(vec![
            ("key1".to_string(), Value::String("val1".to_string())),
            ("key2".to_string(), Value::Int(100)),
        ]);
        assert_eq!(map_val.get("key1").and_then(|v| v.as_str()), Some("val1"));
        assert_eq!(map_val.get("key2").and_then(|v| v.as_i64()), Some(100));
        assert!(map_val.get("nonexistent").is_none());

        if let Some(v) = map_val.get_mut("key2") {
            *v = Value::Int(200);
        }
        assert_eq!(map_val.get("key2").and_then(|v| v.as_i64()), Some(200));
    }

    #[test]
    fn test_element_helpers() {
        let el = Element::new(Sigil::Type("note".to_string()))
            .with_args(Value::Map(vec![("key".to_string(), Value::Bool(true))]))
            .with_content(vec![Inline::Text(Text::from("test"))]);

        assert_eq!(el.name(), Some("note"));
        assert!(!el.is_bare());
        assert!(el.args.is_some());
        assert!(el.content.is_some());

        let bare_item = Element::list_item(
            vec![Inline::Text(Text::from("item"))],
            None,
            None,
            vec![],
            Span::dummy(),
        );
        assert!(bare_item.is_bare());
        assert_eq!(bare_item.name(), None);
    }
}
