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
    /// kind is inferred from a key inside its `input` group via
    /// [`infer_at_kind`] (e.g. `@(url:...)` is a link because `url` is a
    /// recognized link key).
    At(Option<String>),
    /// No sigil at all. Only legal as an entry inside another element's
    /// `ElementValue::Children` (e.g. the `(1)[...]` entries inside
    /// `@links{ ... }`), where the container already supplies the type.
    Bare,
}

/// Keys recognized in a bare `@(key:...)` element's `input` map, checked
/// in this order, to infer its kind when no explicit `@name` is given.
///
/// This exists only for `url`/`file`/`ref`, which need to work *inline* in
/// running prose where every character counts -- it's not a general
/// "authors may omit the name" convenience. `meta` is deliberately **not**
/// here: `@meta(format:tag){...}` is always block-level, never needs to be
/// terse, so it always takes the explicit name. (A bare `@(meta:yaml){...}`
/// would otherwise look like it means the same thing as
/// `@meta(format:yaml){...}` but silently not be: real JSON/YAML/TOML
/// parsing is driven by a `format` key in `input`, checked independently of
/// the sigil/name -- see `typedmark_parser`'s `embedded_format` module --
/// and a bare `@(meta:yaml)`'s `input` has no such key.) `links` is
/// explicit-name-only for the same reason `meta` now is -- it was never in
/// this list.
pub const INFERRED_AT_KEYS: [&str; 3] = ["url", "file", "ref"];

/// Infers a bare `@(...)` element's kind: whichever of [`INFERRED_AT_KEYS`]
/// appears first as a top-level key in `input`. `None` if `input` isn't a
/// map, or none of those keys are present (the caller then typically falls
/// back to a generic "at" kind).
pub fn infer_at_kind(input: Option<&Value>) -> Option<&'static str> {
    let Some(Value::Map(entries)) = input else {
        return None;
    };
    INFERRED_AT_KEYS
        .iter()
        .find(|key| entries.iter().any(|(k, _)| k == *key))
        .copied()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_url_file_ref_from_a_bare_at_element() {
        for key in ["url", "file", "ref"] {
            let input = Value::Map(vec![(key.to_string(), Value::String("x".into()))]);
            assert_eq!(infer_at_kind(Some(&input)), Some(key));
        }
    }

    #[test]
    fn does_not_infer_meta_from_a_bare_at_element() {
        let input = Value::Map(vec![("meta".to_string(), Value::String("yaml".into()))]);
        assert_eq!(infer_at_kind(Some(&input)), None);
    }

    #[test]
    fn no_recognized_key_or_non_map_input_infers_nothing() {
        assert_eq!(infer_at_kind(None), None);
        assert_eq!(infer_at_kind(Some(&Value::String("x".into()))), None);
        let input = Value::Map(vec![("other".to_string(), Value::Int(1))]);
        assert_eq!(infer_at_kind(Some(&input)), None);
    }
}
