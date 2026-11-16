//! The structure of a document: a [`File`] of [`Block`]s, and the [`Inline`]s of a paragraph.

use crate::{
    BodyView, Connect, Data, Declaration, Element, ElementView, Hole, Id, Interpolation, Name,
    Placement, TypeSeat,
};
use serde::{Deserialize, Serialize};
use tomet_span::Span;

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

/// A Tomet file: its `::name(...)` declarations in the preamble header, followed by its body.
///
/// For prose documents (`.tmt`), `body` defaults to `Vec<Block>`.
/// For structured data files (`.ctmt`), `body` is [`Value`](crate::Value).
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
