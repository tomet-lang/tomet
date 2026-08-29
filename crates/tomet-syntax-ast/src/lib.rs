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
