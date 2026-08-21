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
    /// Child blocks nested under this item (e.g., sub-lists or indented blocks).
    pub children: Vec<Block>,
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
            children: Vec::new(),
            span,
        }
    }

    pub fn with_children(
        content: Vec<Inline>,
        marker: Option<String>,
        attrs: Option<Value>,
        children: Vec<Block>,
        span: Span,
    ) -> Self {
        Self {
            content,
            marker,
            attrs,
            children,
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
    /// `typedmark_semantics::infer_at_kind` (e.g. `@(url:...)` is a link
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
/// function is `typedmark-resolve`/`typedmark-compute`'s job, not this
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
