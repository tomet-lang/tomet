//! The nexuses and what they carry: the [`Element`] (`@name`), the [`Declaration`] and [`Connect`] (`::name`) and the [`Hole`],
//! the [`Data`] and [`Group`]s they share, and the [`Name`], [`Id`] and [`Placement`] they hold.

use crate::{Block, Tuple, Value};
use serde::{Deserialize, Serialize};
use std::fmt;
use tomet_span::Span;

/// An element's name, split into its optional namespace and its own name.
///
/// The separator is `.`: `deck.bookmark` is the namespace `deck` and the name
/// `bookmark`. Exactly two namespaces may be
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
/// here -- unlike in entry names, where `url.wiki` is one flat name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
/// `(args)[content]{value}` groups and before any `::name(...)` connect.
///
/// A plain `String`, not [`Value`]: `#(...)` is parsed with a narrower,
/// scalar-only grammar (a bare identifier-like token or a quoted
/// string) than the general value grammar `(args)`/`{value}` use, so a
/// tuple or call can never end up here -- unlike `Name`, there is no
/// namespace concept for an id. No ASCII restriction, unlike [`Name`]:
/// the content is a value, not a structural identifier, so it is as
/// free as any other string value (e.g. `@line(天音かなた)` elsewhere in
/// this codebase already allows non-ASCII value text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
/// `content` is a list of blocks: an element written at a line start inside
/// another element's `[content]` is a block, and this field is the only
/// place that survives.
///
/// This is placement, not shape. Which shape an element is *allowed* to
/// take is a vocabulary question, answered by `tomet-semantics`'
/// `required_shape` and checked against this field by `shape_mismatch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
/// ([`File::header`](crate::File::header)), the run of `::` declarations at the top -- so the preamble can be
/// found without loading any vocabulary. Which names may be written this way is the
/// `Region` a vocabulary declares for them, and whether one takes `[content]`
/// is that vocabulary's call too, so the parser does not decide it.
///
/// It sits in [`File::header`](crate::File::header): a `::` has to open its line and stand alone on it,
/// and so has no [`Placement`]. And it is attached to nothing, so unlike an
/// [`Element`] it takes no `::connect`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

/// A `<?matcher(name, args){options}>[content]` hole: the description of a value that is
/// not there yet, in a pattern.
///
/// It is a box, `<...>`, whose head (the `?` mark, the matcher and the `(args)` and `{options}`
/// groups) is inside and whose `[content]` follows the `>`. Its groups are read by the
/// same code as an element's. Differences: its head is a *matcher*, not an element name, and it is
/// optional (`<?>` and `<?(n)>` have none); and it takes no `::connect`. A box that does not close
/// or does not fit is text, so ordinary prose such as `本当に?(はい)` or `a < b` is never a hole.
///
/// A hole sits where an element sits, so it is a variant of [`Block`], [`Inline`](crate::Inline), [`Entry`]
/// and [`Value`], in the same tree a document is: a pattern is written in, and printed as,
/// ordinary tomet, and `parse_document` reads one like any other construct.
///
/// The matcher is not an element name: `<?meta>` is not the `meta` element. What a matcher
/// means, and what a hole matches, is the host's (tandr's), not tomet's. See
/// `tmtroot/spec/syntax/hole.tmt`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

/// A `::name(...)` connect stacked after an element, a section or a list
/// (`@x(...)::as(y)::rule(...)`): metadata about its owner, not content of the
/// document. It takes `(args)`, `[content]` and `{value}` like an element does,
/// and nothing else: no `#(id)` (an id belongs to the owner, written before the
/// first connect), no connects of its own, and no placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

/// The data a node carries: its `(args)` tuple and its `{value}` group, each
/// optional and at most one of each.
///
/// Every node that takes either holds them as a `Data`, so what reads them
/// (the walkers, the printer, `merge_values`) is written once for all of
/// them. `[content]` and `#(id)` are not in here on purpose: they differ in
/// kind (child blocks, an address) and not every node takes them.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Data {
    pub args: Option<Tuple>,
    pub value: Option<Group>,
}

/// An `@name(args)[content]{value}` element.
///
/// The one element sigil is `@`; it says "an element starts here" and nothing
/// else. Whether the element stands as a block or belongs to a paragraph is
/// [`Placement`], which the parser derives from position, and origin is
/// carried by [`Name`]'s namespace. Two other axes would carry no
/// information: `<T>` vs `@name` (official vs user-defined, but both
/// classified through the same arm) and `#name` vs `@name` (shape, which
/// `tomet-semantics`' `required_shape` already knew). `#(id)` is not a second
/// sigil either: it is [`Element::id`], a slot in the element's own groups.
///
/// A node that has no name of its own, a plain list item, is not an element at all.
///
/// `(args)` / `[content]` / `{value}`, each optional and at most one of each,
/// in any order in the source.
///
/// `id` is a single slot, written `#(foobar)`, read after those three
/// groups but strictly before any connect -- see [`Id`].
///
/// `connects` is unrelated to those three: it is the `::name(...)` family
/// stacked after them (`@x(...)::as(y)::rule(...)`), zero or more, each a
/// [`Connect`]. Order-preserving, empty when nothing follows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Element {
    /// The name after `@`. It is mandatory: a nameless `@(url:...)` is not an element,
    /// because a kind is not inferred from a key in the args.
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
/// combine list item `-@name(...)` via [`ListItem::as_element_view`](crate::ListItem::as_element_view)) without
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
    pub fn args(&self) -> Option<&'a Tuple> {
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
/// [`Connect`], [`Hole`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyView<'a> {
    pub data: &'a Data,
    pub content: Option<&'a [Block]>,
    pub id: Option<&'a Id>,
}

impl<'a> BodyView<'a> {
    /// Positional or keyword arguments `(args)`.
    pub fn args(&self) -> Option<&'a Tuple> {
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
/// `Pair` is a named entry (`key: value`), which lets the data view be
/// rebuilt as a [`Tuple`] from a group without copying any other structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Entry {
    Pair(String, Value),
    /// A named entry, `@name(...)`.
    Element(Element),
    /// A `<?matcher(...)>` hole standing as an entry. See [`Hole`].
    Hole(Hole),
}

/// The entries of a `{...}` group, in source order.
///
/// `{...}` is always data: it parses uniformly into [`Entry`] items in
/// source order, whatever the element is called. "Is this a data group or a
/// list of children?" is not a parse-time branch -- it is a view
/// computed downstream in `tomet-semantics`, which is what lets the parser
/// build the tree without consulting any element vocabulary.
///
/// A group holds a source-order sequence of entries
/// and provides named-entry operations exclusively for [`Entry::Pair`] entries.
/// Non-pair entries ([`Entry::Element`], [`Entry::Hole`]) are
/// handled directly by callers through exhaustive pattern matching.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Group(pub Vec<Entry>);

impl Group {
    /// Builds a group from the named entries of `tuple`.
    pub fn from_map(tuple: Tuple) -> Self {
        Group(tuple.into_named().map(|(k, v)| Entry::Pair(k, v)).collect())
    }

    /// The data view: this group's `key: value` pairs, in source order,
    /// as a [`Tuple`]. Non-pair entries are skipped.
    pub fn as_data(&self) -> Tuple {
        let Group(entries) = self;
        Tuple::from_named(entries.iter().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k.clone(), v.clone())),
            Entry::Element(_) | Entry::Hole(_) => None,
        }))
    }

    /// This group's `key: value` pairs, in source order.
    pub fn pairs(&self) -> impl Iterator<Item = (&String, &Value)> {
        let Group(entries) = self;
        entries.iter().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k, v)),
            Entry::Element(_) | Entry::Hole(_) => None,
        })
    }

    /// Mutable counterpart of [`Group::pairs`].
    pub fn pairs_mut(&mut self) -> impl Iterator<Item = (&mut String, &mut Value)> {
        let Group(entries) = self;
        entries.iter_mut().filter_map(|e| match e {
            Entry::Pair(k, v) => Some((k, v)),
            Entry::Element(_) | Entry::Hole(_) => None,
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
            Entry::Element(_) | Entry::Hole(_) => {
                unreachable!("position matched a Pair")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Inline, ListItem, Paragraph, Text};

    #[test]
    fn test_element_view_and_list_item_as_element_view() {
        let name = Name::bare("item");
        let args_tuple = Tuple::from_unnamed([Value::Scalar(tomet_scalar::Scalar::Int(42))]);
        let data = Data {
            args: Some(args_tuple.clone()),
            value: Some(Group::from_map(Tuple::from_named([(
                "key".to_string(),
                Value::Scalar(tomet_scalar::Scalar::Text("val".to_string())),
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
        assert_eq!(el_view.args(), Some(&args_tuple));
        assert!(el_view.value().is_some());
        assert_eq!(el_view.id, Some(&id));
        assert_eq!(el_view.connects, &connects);

        // Body view matches
        let body = el_view.body();
        assert_eq!(body, item.as_body_view());
        assert_eq!(body, element.as_body_view());
        assert_eq!(body.args(), Some(&args_tuple));
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

        // BodyView on Declaration, Hole, Connect
        let decl = Declaration {
            name: name.clone(),
            data: data.clone(),
            content: Some(content_blocks.clone()),
            id: Some(id.clone()),
            span: Span::dummy(),
        };
        assert_eq!(decl.as_body_view(), body);

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
