//! What sits inside what: the [`Node`] a walker meets, the [`Place`] it sits in, and the one recursion over them.

use std::ops::ControlFlow;
use tomet_ast::{
    Block, Connect, Data, Declaration, Element, ElementView, Entry, File, Group, Hole, Id, Inline,
    Interpolation, List, ListItem, Section, TypeSeat, Value,
};
use tomet_span::Span;

/// Where a node sits in the node that holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A declaration in the file preamble header.
    Header,
    /// A block of the file itself.
    Top,
    /// An inline of a paragraph.
    Text,
    /// An inline of a section's title.
    Title,
    /// A block of a section's body.
    Body,
    /// The value of `(args)`.
    Args,
    /// An entry of a `{value}` group.
    Entry,
    /// The value of a `key: value` entry.
    PairValue,
    /// An item of a `(...)`, a map's value or a call's argument.
    Part,
    /// A block of a `[content]`, or of a `[...]` value.
    Content,
    /// A `::connect`.
    Connect,
    /// An item of a list.
    Item,
    /// The list nested in a list item.
    Sublist,
}

/// A borrowed reference to any vertex of a file's AST ([`Declaration`], [`Block`], [`Inline`],
/// [`Value`], [`Entry`], [`Connect`], [`ListItem`], [`List`]).
///
/// Unlike `tomet_expr::Part`, which is an owned runtime value holding only a structured part of a
/// document during expression evaluation, [`Node`] is the universal borrowed cursor over the whole
/// syntax tree.
///
/// An element is not a node of its own: it is the `Block`, `Inline`, `Value` or `Entry` it stands
/// as (see [`Node::element`] and [`Node::element_view`]). A list at the top of a file is a `Block`;
/// the list nested in an item is a [`Node::List`].
#[derive(Debug, Clone, Copy)]
pub enum Node<'a> {
    Declaration(&'a Declaration),
    Block(&'a Block),
    Inline(&'a Inline),
    Value(&'a Value),
    /// An entry of a `{...}` group: a `key: value` pair, an element or a hole.
    Entry(&'a Entry),
    Connect(&'a Connect),
    Item(&'a ListItem),
    /// A list nested in a list item.
    List(&'a List),
}

impl<'a> Node<'a> {
    /// The element this node is, wherever it stands.
    pub fn element(self) -> Option<&'a Element> {
        match self {
            Node::Block(Block::Element(el))
            | Node::Inline(Inline::Element(el))
            | Node::Entry(Entry::Element(el)) => Some(el),
            Node::Value(Value::Element(el)) => Some(el),
            _ => None,
        }
    }

    /// The element view of this node, wherever it stands.
    pub fn element_view(self) -> Option<ElementView<'a>> {
        match self {
            Node::Block(Block::Element(el))
            | Node::Inline(Inline::Element(el))
            | Node::Entry(Entry::Element(el)) => Some(el.as_view()),
            Node::Value(Value::Element(el)) => Some(el.as_view()),
            Node::Item(item) => item.as_element_view(),
            _ => None,
        }
    }

    /// The span of whichever AST vertex this node wraps.
    pub fn span(self) -> Option<Span> {
        match self {
            Node::Declaration(d) => Some(d.span),
            Node::Block(b) => Some(b.span()),
            Node::Inline(i) => Some(i.span()),
            Node::Connect(c) => Some(c.span),
            Node::Item(item) => Some(item.span),
            Node::List(l) => Some(l.span),
            Node::Entry(Entry::Element(e)) => Some(e.span),
            Node::Entry(Entry::Hole(h)) => Some(h.span),
            Node::Entry(Entry::Pair(..)) => None,
            Node::Value(Value::Element(e)) => Some(e.span),
            Node::Value(Value::Hole(h)) => Some(h.span),
            Node::Value(Value::Interp(it)) => Some(it.span),
            Node::Value(Value::TypeSeat(t)) => Some(t.span),
            Node::Value(Value::Call(_, args)) => (!args.span().is_dummy()).then_some(args.span()),
            Node::Value(_) => None,
        }
    }

    /// The `Value::Call` this node is, with its arguments and source span.
    pub fn call(self) -> Option<(&'a str, &'a tomet_ast::Tuple, Span)> {
        match self {
            Node::Value(Value::Call(name, args)) => Some((name.as_str(), args, args.span())),
            _ => None,
        }
    }

    /// The hole this node is, wherever it stands.
    pub fn hole(self) -> Option<&'a Hole> {
        match self {
            Node::Block(Block::Hole(h))
            | Node::Inline(Inline::Hole(h))
            | Node::Entry(Entry::Hole(h)) => Some(h),
            Node::Value(Value::Hole(h)) => Some(h),
            _ => None,
        }
    }

    pub fn declaration(self) -> Option<&'a Declaration> {
        match self {
            Node::Declaration(d) => Some(d),
            _ => None,
        }
    }

    pub fn section(self) -> Option<&'a Section> {
        match self {
            Node::Block(Block::Section(s)) => Some(s),
            _ => None,
        }
    }

    /// The list this node is: one standing as a block, or one nested in an item.
    pub fn list(self) -> Option<&'a List> {
        match self {
            Node::Block(Block::List(l)) | Node::List(l) => Some(l),
            _ => None,
        }
    }

    /// The `#(id)` this node carries, if it is a kind that can: an element, a declaration, a
    /// section, a list or a list item.
    pub fn id(self) -> Option<&'a Id> {
        match self {
            Node::Declaration(d) => d.id.as_ref(),
            Node::Block(Block::Section(s)) => s.id.as_ref(),
            Node::Block(Block::List(l)) | Node::List(l) => l.id.as_ref(),
            Node::Item(item) => item.id.as_ref(),
            other => other.element().and_then(|el| el.id.as_ref()),
        }
    }

    pub fn interpolation(self) -> Option<&'a Interpolation> {
        match self {
            Node::Block(Block::Interp(i))
            | Node::Inline(Inline::Interp(i))
            | Node::Value(Value::Interp(i)) => Some(i),
            _ => None,
        }
    }

    /// The type seat this node is, wherever it stands.
    pub fn type_seat(self) -> Option<&'a TypeSeat> {
        match self {
            Node::Block(Block::TypeSeat(t))
            | Node::Inline(Inline::TypeSeat(t))
            | Node::Value(Value::TypeSeat(t)) => Some(t),
            _ => None,
        }
    }
}

/// What a walk does after it has been shown a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visit<B = ()> {
    /// Go on into what the node holds.
    Descend,
    /// Do not look at what the node holds; go on with its siblings.
    Skip,
    /// Stop the whole walk, handing `B` back.
    Break(B),
}

/// Calls `f` for every node sitting directly in `node`, with the [`Place`] it sits in.
///
/// This is the one place that says what is inside what. A section holds its title, its
/// `{value}`, its body and its connects; a node that carries data holds `(args)`, `{value}`
/// entries and `[content]` blocks; an element or a hole inside a block, a line of text, a value
/// or an entry holds what the element or hole holds.
#[deny(clippy::wildcard_enum_match_arm)]
pub fn children<'a>(node: Node<'a>, f: &mut dyn FnMut(Node<'a>, Place)) {
    match node {
        Node::Declaration(d) => groups(&d.data, d.content.as_deref(), f),
        Node::Block(block) => match block {
            Block::Paragraph(p) => inlines(&p.content, Place::Text, f),
            Block::Element(el) => element(el, f),
            Block::Hole(h) => groups(&h.data, h.content.as_deref(), f),
            Block::Section(s) => {
                inlines(&s.title, Place::Title, f);
                data(&s.data, f);
                for b in &s.blocks {
                    f(Node::Block(b), Place::Body);
                }
                connects(&s.connects, f);
            }
            Block::List(l) => list(l, f),
            Block::Interp(_) | Block::TypeSeat(_) => {}
        },
        Node::Inline(inline) => match inline {
            Inline::Element(el) => element(el, f),
            Inline::Hole(h) => groups(&h.data, h.content.as_deref(), f),
            Inline::Text(_)
            | Inline::Raw(_)
            | Inline::SoftBreak(_)
            | Inline::LineBreak(_)
            | Inline::Interp(_)
            | Inline::TypeSeat(_) => {}
        },
        Node::Value(value) => match value {
            Value::Element(el) => element(el, f),
            Value::Hole(h) => groups(&h.data, h.content.as_deref(), f),
            Value::Tuple(tuple) | Value::Call(_, tuple) => {
                for v in tuple.values() {
                    f(Node::Value(v), Place::Part);
                }
            }
            Value::Blocks(blocks) => {
                for b in blocks {
                    f(Node::Block(b), Place::Content);
                }
            }
            Value::Scalar(_) | Value::Interp(_) | Value::TypeSeat(_) => {}
        },
        Node::Entry(entry) => match entry {
            Entry::Pair(_, v) => f(Node::Value(v), Place::PairValue),
            Entry::Element(el) => element(el, f),
            Entry::Hole(h) => groups(&h.data, h.content.as_deref(), f),
        },
        Node::Connect(c) => groups(&c.data, c.content.as_deref(), f),
        Node::Item(item) => {
            groups(&item.data, Some(&item.content), f);
            if let Some(sub) = &item.sublist {
                f(Node::List(sub), Place::Sublist);
            }
            connects(&item.connects, f);
        }
        Node::List(l) => list(l, f),
    }
}

fn inlines<'a>(items: &'a [Inline], place: Place, f: &mut dyn FnMut(Node<'a>, Place)) {
    for inline in items {
        f(Node::Inline(inline), place);
    }
}

fn connects<'a>(items: &'a [Connect], f: &mut dyn FnMut(Node<'a>, Place)) {
    for connect in items {
        f(Node::Connect(connect), Place::Connect);
    }
}

fn list<'a>(l: &'a List, f: &mut dyn FnMut(Node<'a>, Place)) {
    for item in &l.items {
        f(Node::Item(item), Place::Item);
    }
    connects(&l.connects, f);
}

fn element<'a>(el: &'a Element, f: &mut dyn FnMut(Node<'a>, Place)) {
    groups(&el.data, el.content.as_deref(), f);
    connects(&el.connects, f);
}

/// `(args)`, `{value}` and `[content]`: what every node that carries data has in common.
pub(super) fn groups<'a>(
    d: &'a Data,
    content: Option<&'a [Block]>,
    f: &mut dyn FnMut(Node<'a>, Place),
) {
    data(d, f);
    for block in content.into_iter().flatten() {
        f(Node::Block(block), Place::Content);
    }
}

/// `(args)` and `{value}`.
fn data<'a>(d: &'a Data, f: &mut dyn FnMut(Node<'a>, Place)) {
    if let Some(args) = &d.args {
        for v in args.values() {
            f(Node::Value(v), Place::Args);
        }
    }
    if let Some(Group(entries)) = &d.value {
        for entry in entries {
            f(Node::Entry(entry), Place::Entry);
        }
    }
}

/// Shows `f` every node of `file`, each before what it holds, in document order, and goes into
/// what `f` answers [`Visit::Descend`] to. [`Visit::Break`] stops the walk and is handed back.
pub fn walk<'a, B>(
    file: &'a File,
    f: &mut impl FnMut(Node<'a>, Place) -> Visit<B>,
) -> ControlFlow<B> {
    for decl in &file.header {
        walk_node(Node::Declaration(decl), Place::Header, f)?;
    }
    for block in &file.body {
        walk_node(Node::Block(block), Place::Top, f)?;
    }
    ControlFlow::Continue(())
}

/// [`walk`] from one node, which sits in `place`.
pub fn walk_node<'a, B>(
    node: Node<'a>,
    place: Place,
    f: &mut impl FnMut(Node<'a>, Place) -> Visit<B>,
) -> ControlFlow<B> {
    walk_node_scoped(node, place, f, &mut |_, _| {})
}

/// [`walk_node`] that also says when it is done with a node.
///
/// `enter` is `walk_node`'s `f`: shown each node before what it holds, and answering what to do
/// next. `leave` is shown the same node and place once everything the node holds has been walked.
/// It is called for a node `enter` answered [`Visit::Descend`] to and for one it answered
/// [`Visit::Skip`] to (entered, and just holding nothing the walk looked at), and not for one it
/// answered [`Visit::Break`] to, because the walk ends there.
///
/// This is for a walker whose answer depends on where a node ends. The text of a run of blocks
/// puts a blank line between two blocks and none between a line of text and the paragraph an
/// inline element holds; a visitor that is only shown entries cannot tell which paragraph it is
/// in, and could only guess it from the order it was shown things in. This is still the one
/// recursion: [`walk_node`] is this with a `leave` that does nothing.
pub fn walk_node_scoped<'a, B>(
    node: Node<'a>,
    place: Place,
    enter: &mut impl FnMut(Node<'a>, Place) -> Visit<B>,
    leave: &mut impl FnMut(Node<'a>, Place),
) -> ControlFlow<B> {
    match enter(node, place) {
        Visit::Break(b) => return ControlFlow::Break(b),
        Visit::Skip => {
            leave(node, place);
            return ControlFlow::Continue(());
        }
        Visit::Descend => {}
    }
    let mut flow = ControlFlow::Continue(());
    children(node, &mut |child, at| {
        if flow.is_continue() {
            flow = walk_node_scoped(child, at, enter, leave);
        }
    });
    if flow.is_continue() {
        leave(node, place);
    }
    flow
}
