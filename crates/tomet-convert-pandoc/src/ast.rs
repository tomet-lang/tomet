//! Pandoc's own AST, as serde types matching its JSON encoding.
//!
//! This is a transcription of `Text.Pandoc.Definition`, not a design of
//! ours -- every name and shape here is dictated by what
//! `pandoc -f json` accepts and `pandoc -t json` emits.
//!
//! Two encoding rules cover almost all of it:
//!
//! - a node is `{"t": "TagName", "c": <payload>}`, and a node with no
//!   payload omits `c` entirely (`{"t":"Space"}`);
//! - a constructor taking several arguments encodes them as a JSON array
//!   in declaration order, which is why the variants below hold tuples.

use serde::{Deserialize, Serialize};

/// A whole document: the API version, the metadata map, and the body.
///
/// `pandoc_api_version` must match the Pandoc binary reading it, or the
/// input is rejected. See [`PANDOC_API_VERSION`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PandocDoc {
    #[serde(rename = "pandoc-api-version")]
    pub pandoc_api_version: Vec<u32>,
    pub meta: Meta,
    pub blocks: Vec<Block>,
}

/// The version this crate writes, and the only one it is known to be
/// compatible with.
///
/// Pandoc compares major and minor and refuses input that disagrees, so
/// this needs to follow Pandoc releases. The check lives in
/// `tests/api_version.rs`, which runs only where a `pandoc` binary is
/// actually installed.
pub const PANDOC_API_VERSION: [u32; 3] = [1, 23, 1];

impl PandocDoc {
    pub fn new(blocks: Vec<Block>) -> Self {
        Self {
            pandoc_api_version: PANDOC_API_VERSION.to_vec(),
            meta: Meta::default(),
            blocks,
        }
    }
}

/// The document metadata map -- what becomes YAML frontmatter, `\title{}`,
/// docx document properties, and so on, depending on the output format.
pub type Meta = std::collections::BTreeMap<String, MetaValue>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum MetaValue {
    MetaString(String),
    MetaBool(bool),
    MetaList(Vec<MetaValue>),
    MetaMap(std::collections::BTreeMap<String, MetaValue>),
    MetaInlines(Vec<Inline>),
    MetaBlocks(Vec<Block>),
}

/// `(identifier, classes, key-value pairs)`.
///
/// Values are plain strings with no structure of their own, which is what
/// forces the flattening in `tomet_semantics::flatten`: a nested Tomet
/// `{value}` has no direct representation here.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Attr(pub String, pub Vec<String>, pub Vec<(String, String)>);

impl Attr {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn with_class(class: impl Into<String>) -> Self {
        Attr(String::new(), vec![class.into()], Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty() && self.1.is_empty() && self.2.is_empty()
    }
}

/// `(url, title)`. The title is usually empty.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Target(pub String, pub String);

impl Target {
    pub fn url(url: impl Into<String>) -> Self {
        Target(url.into(), String::new())
    }
}

/// `(start number, style, delimiter)` on an ordered list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListAttributes(pub i64, pub ListNumberStyle, pub ListNumberDelim);

impl Default for ListAttributes {
    fn default() -> Self {
        ListAttributes(1, ListNumberStyle::Decimal, ListNumberDelim::Period)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ListNumberStyle {
    DefaultStyle,
    Example,
    Decimal,
    LowerRoman,
    UpperRoman,
    LowerAlpha,
    UpperAlpha,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum ListNumberDelim {
    DefaultDelim,
    Period,
    OneParen,
    TwoParens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum Block {
    Plain(Vec<Inline>),
    Para(Vec<Inline>),
    LineBlock(Vec<Vec<Inline>>),
    CodeBlock(Attr, String),
    RawBlock(String, String),
    BlockQuote(Vec<Block>),
    OrderedList(ListAttributes, Vec<Vec<Block>>),
    BulletList(Vec<Vec<Block>>),
    DefinitionList(Vec<(Vec<Inline>, Vec<Vec<Block>>)>),
    Header(i64, Attr, Vec<Inline>),
    HorizontalRule,
    /// Boxed only to keep `Block` small -- a table is by far the widest
    /// variant and every other block would pay for it. `TableParts` is a
    /// tuple struct, so it still encodes as the positional array Pandoc
    /// expects; see `a_table_still_encodes_as_a_positional_array`.
    Table(Box<TableParts>),
    Figure(Attr, Caption, Vec<Block>),
    Div(Attr, Vec<Block>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum Inline {
    Str(String),
    Emph(Vec<Inline>),
    Underline(Vec<Inline>),
    Strong(Vec<Inline>),
    Strikeout(Vec<Inline>),
    Superscript(Vec<Inline>),
    Subscript(Vec<Inline>),
    SmallCaps(Vec<Inline>),
    Quoted(QuoteType, Vec<Inline>),
    Code(Attr, String),
    Space,
    SoftBreak,
    LineBreak,
    Math(MathType, String),
    RawInline(String, String),
    Link(Attr, Vec<Inline>, Target),
    Image(Attr, Vec<Inline>, Target),
    Note(Vec<Block>),
    Span(Attr, Vec<Inline>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum QuoteType {
    SingleQuote,
    DoubleQuote,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum MathType {
    DisplayMath,
    InlineMath,
}

// ---- table parts ---------------------------------------------------
//
// Pandoc's table is far richer than Tomet's: a caption, per-column specs,
// and a head/bodies/foot split whose rows carry span information. Tomet
// tables have none of that, so writing one is mostly filling in defaults
// -- but the shapes still have to be exact for Pandoc to accept the JSON.

/// `Table`'s payload: `(attr, caption, column specs, head, bodies, foot)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableParts(
    pub Attr,
    pub Caption,
    pub Vec<ColSpec>,
    pub TableHead,
    pub Vec<TableBody>,
    pub TableFoot,
);

/// `(short caption, caption blocks)`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Caption(pub Option<Vec<Inline>>, pub Vec<Block>);

/// `(alignment, column width)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColSpec(pub Alignment, pub ColWidth);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Alignment {
    AlignLeft,
    AlignRight,
    AlignCenter,
    AlignDefault,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum ColWidth {
    ColWidth(f64),
    ColWidthDefault,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableHead(pub Attr, pub Vec<Row>);

/// `(attr, row head columns, header rows, body rows)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableBody(pub Attr, pub RowHeadColumns, pub Vec<Row>, pub Vec<Row>);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableFoot(pub Attr, pub Vec<Row>);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowHeadColumns(pub i64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row(pub Attr, pub Vec<Cell>);

/// `(attr, alignment, rowspan, colspan, contents)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cell(pub Attr, pub Alignment, pub i64, pub i64, pub Vec<Block>);

#[cfg(test)]
mod tests {
    use super::*;

    /// The encoding rules the rest of this crate relies on: a payload-less
    /// node has no `c`, and a multi-argument one is a positional array.
    #[test]
    fn nodes_encode_the_way_pandoc_writes_them() {
        assert_eq!(
            serde_json::to_string(&Inline::Space).unwrap(),
            r#"{"t":"Space"}"#
        );
        assert_eq!(
            serde_json::to_string(&Inline::Str("x".into())).unwrap(),
            r#"{"t":"Str","c":"x"}"#
        );
        assert_eq!(
            serde_json::to_string(&Block::Header(
                2,
                Attr(
                    "id".into(),
                    vec!["c".into()],
                    vec![("k".into(), "v".into())]
                ),
                vec![Inline::Str("T".into())],
            ))
            .unwrap(),
            r#"{"t":"Header","c":[2,["id",["c"],[["k","v"]]],[{"t":"Str","c":"T"}]]}"#
        );
    }

    #[test]
    fn a_table_still_encodes_as_a_positional_array() {
        // The `Box` is a memory-layout concern only. Pandoc reads `c` as
        // the constructor's arguments in order, so it has to stay an
        // array, not become an object.
        let table = Block::Table(Box::new(TableParts(
            Attr::empty(),
            Caption::default(),
            vec![],
            TableHead(Attr::empty(), vec![]),
            vec![],
            TableFoot(Attr::empty(), vec![]),
        )));
        let json = serde_json::to_string(&table).unwrap();
        assert!(json.starts_with(r#"{"t":"Table","c":["#), "got: {json}");
        assert_eq!(serde_json::from_str::<Block>(&json).unwrap(), table);
    }

    #[test]
    fn a_document_round_trips_through_json() {
        let doc = PandocDoc::new(vec![Block::Para(vec![Inline::Str("hi".into())])]);
        let json = serde_json::to_string(&doc).unwrap();
        assert!(json.starts_with(r#"{"pandoc-api-version":[1,23,1],"meta":{},"#));
        assert_eq!(serde_json::from_str::<PandocDoc>(&json).unwrap(), doc);
    }
}
