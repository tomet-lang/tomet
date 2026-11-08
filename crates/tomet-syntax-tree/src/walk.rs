//! One definition of what sits inside what in a [`File`], and the walkers built on it.
//!
//! [`children`] says, for any [`Node`], which nodes sit directly inside it and in which [`Place`].
//! [`walk`] is the one recursion over that, through [`walk_node_scoped`], which also says when a
//! node is done with, for a walker that must know where a block ends. Every other walker here is [`walk`] with a filter
//! and, where a position must stay unseen, a [`Visit::Skip`] written at the walker, so what a
//! walker leaves out is read in its own body and not in a copy of the recursion.
//!
//! Traversal must never be reimplemented by handwritten AST recursion across downstream crates.
//! All structural walks across the workspace route through [`children`] and [`walk`].
//!
//! The groups of a node come in one order: `(args)`, `{value}`, `[content]`, then its
//! `:connect`s. The AST keeps no order of groups, so this is the order of every walker.
//!
//! The one exception to "one recursion" is the mutable walk ([`for_each_element_mut`],
//! [`for_each_id_mut`], [`fill_interpolations`]): a mutable walk cannot share the borrow with
//! [`children`], so it lists the children again, and `tests/walk_coverage.rs` holds the two to the
//! same reach.

use std::ops::ControlFlow;
use tomet_ast::{
    Block, Connect, Data, Declaration, Element, ElementView, Entry, File, Group, Hole, Id, Inline,
    Interpolation, List, ListItem, Paragraph, Section, Text, Value,
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
    /// A `:connect`.
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
    /// An entry of a `{...}` group: a `key: value` pair, an element, a hole or a bare entry.
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
            Node::Entry(Entry::Bare(b)) => Some(b.span),
            Node::Entry(Entry::Pair(..)) => None,
            Node::Value(Value::Element(e)) => Some(e.span),
            Node::Value(Value::Hole(h)) => Some(h.span),
            Node::Value(Value::Interp(it)) => Some(it.span),
            Node::Value(_) => None,
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
    /// section, a list, a list item or a bare entry.
    pub fn id(self) -> Option<&'a Id> {
        match self {
            Node::Declaration(d) => d.id.as_ref(),
            Node::Block(Block::Section(s)) => s.id.as_ref(),
            Node::Block(Block::List(l)) | Node::List(l) => l.id.as_ref(),
            Node::Item(item) => item.id.as_ref(),
            Node::Entry(Entry::Bare(b)) => b.id.as_ref(),
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
            Block::Interp(_) => {}
        },
        Node::Inline(inline) => match inline {
            Inline::Element(el) => element(el, f),
            Inline::Hole(h) => groups(&h.data, h.content.as_deref(), f),
            Inline::Text(_)
            | Inline::Raw(_)
            | Inline::SoftBreak(_)
            | Inline::LineBreak(_)
            | Inline::Interp(_) => {}
        },
        Node::Value(value) => match value {
            Value::Element(el) => element(el, f),
            Value::Hole(h) => groups(&h.data, h.content.as_deref(), f),
            Value::Seq(items) | Value::Call(_, items) => {
                for v in items {
                    f(Node::Value(v), Place::Part);
                }
            }
            Value::Map(entries) => {
                for (_, v) in entries {
                    f(Node::Value(v), Place::Part);
                }
            }
            Value::Blocks(blocks) => {
                for b in blocks {
                    f(Node::Block(b), Place::Content);
                }
            }
            Value::Null
            | Value::Interp(_)
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::Number(_)
            | Value::String(_) => {}
        },
        Node::Entry(entry) => match entry {
            Entry::Pair(_, v) => f(Node::Value(v), Place::PairValue),
            Entry::Element(el) => element(el, f),
            Entry::Hole(h) => groups(&h.data, h.content.as_deref(), f),
            Entry::Bare(b) => groups(&b.data, b.content.as_deref(), f),
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
fn groups<'a>(d: &'a Data, content: Option<&'a [Block]>, f: &mut dyn FnMut(Node<'a>, Place)) {
    data(d, f);
    for block in content.into_iter().flatten() {
        f(Node::Block(block), Place::Content);
    }
}

/// `(args)` and `{value}`.
fn data<'a>(d: &'a Data, f: &mut dyn FnMut(Node<'a>, Place)) {
    if let Some(args) = &d.args {
        f(Node::Value(args), Place::Args);
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

/// Visits every node of `file` in document order, each before what it holds, wherever it sits:
/// running text, section titles, `:connect`s, and inside a [`Value`] (`Seq`, `Map`, `Call`, a
/// nested element, `[...]` blocks). It is the walk that skips nothing.
pub fn for_each_node(file: &File, mut f: impl FnMut(Node<'_>)) {
    let _ = walk(file, &mut |node, _| {
        f(node);
        Visit::<()>::Descend
    });
}

/// Every node of `file` that is part of what the document says: [`for_each_node`] without the
/// `:connect`s, which are metadata about their owner and not a part of the document (see
/// [`for_each_descendant`]). The element, list and section walkers are filters over this.
fn for_each_part(file: &File, mut f: impl FnMut(Node<'_>)) {
    let _ = walk(file, &mut |node, place| {
        if place == Place::Connect {
            return Visit::<()>::Skip;
        }
        f(node);
        Visit::Descend
    });
}

/// Traverses all elements in `file` in document order and executes `f` for each.
///
/// An element in a `:connect` is not reached.
pub fn for_each_element(file: &File, mut f: impl FnMut(&Element)) {
    for_each_part(file, |node| {
        if let Some(el) = node.element() {
            f(el);
        }
    });
}

/// Traverses every [`Inline`] in `file` in document order: paragraph content, section titles and
/// every `[content]`. An `Inline::Element` is itself passed to `f`, and then descended into.
pub fn for_each_inline(file: &File, mut f: impl FnMut(&Inline)) {
    for_each_part(file, |node| {
        if let Node::Inline(inline) = node {
            f(inline);
        }
    });
}

/// Visits every [`ListItem`] in `file`, in document order, at any depth: a list item is not an
/// [`Element`], so [`for_each_element`] never passes one.
pub fn for_each_list_item(file: &File, mut f: impl FnMut(&ListItem)) {
    for_each_part(file, |node| {
        if let Node::Item(item) = node {
            f(item);
        }
    });
}

/// Visits every [`List`] in `file`, in document order: those standing as a block, wherever the
/// block is, and the ones nested in a list item.
pub fn for_each_list(file: &File, mut f: impl FnMut(&List)) {
    for_each_part(file, |node| {
        if let Some(list) = node.list() {
            f(list);
        }
    });
}

/// Visits every [`Section`] in `file`, in document order, including ones nested under another
/// section's body or in an element's content: a heading's own `:name(...)` connect is checked
/// against its `blocks` ([`for_each_descendant_in_blocks`]) the way an element's connect is
/// checked against its own content.
pub fn for_each_section(file: &File, mut f: impl FnMut(&Section)) {
    for_each_part(file, |node| {
        if let Some(section) = node.section() {
            f(section);
        }
    });
}

/// Visits every [`Interpolation`] in `file`, in document order, wherever it sits.
///
/// An interpolation is not an [`Element`], so the element walkers never pass one to their
/// visitor; this is the walker to ask for "every `${...}` in the document".
pub fn for_each_interpolation(file: &File, mut f: impl FnMut(&Interpolation)) {
    for_each_node(file, |node| {
        if let Some(interp) = node.interpolation() {
            f(interp);
        }
    });
}

/// Visits every [`Declaration`] in `file`, in document order, wherever it sits.
pub fn for_each_declaration(file: &File, mut f: impl FnMut(&Declaration)) {
    for_each_node(file, |node| {
        if let Some(declaration) = node.declaration() {
            f(declaration);
        }
    });
}

/// Visits every [`Hole`] in `file`, in document order, wherever it sits: as a block, in running
/// text, as an entry of a `{...}` group or as a value.
pub fn for_each_hole(file: &File, mut f: impl FnMut(&Hole)) {
    for_each_node(file, |node| {
        if let Some(hole) = node.hole() {
            f(hole);
        }
    });
}

/// Visits every element a reader would find by scanning only the top of the file, in
/// document order: every `Block::Element`, and every `Inline::Element` sitting directly in a
/// `Block::Paragraph`'s own content (an element that shares its paragraph with other text is
/// still at the top; something nested inside another element is not).
///
/// This is the one place that decision lives. A caller that wants "the elements at the top of
/// this file" should call this instead of walking `file.body` by hand. Declarations are in
/// `file.header`.
#[deny(clippy::wildcard_enum_match_arm)]
pub fn for_each_top_level_element(file: &File, mut f: impl FnMut(&Element)) {
    for block in &file.body {
        match block {
            Block::Element(el) => f(el),
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        f(el);
                    }
                }
            }
            Block::Section(_) | Block::List(_) | Block::Hole(_) | Block::Interp(_) => {}
        }
    }
}

/// What a descendant walk shows `f`, given the nodes at its start. It leaves out three positions,
/// at every depth:
///
/// - `(args)`: an element embedded in an element's own `(args)` --
///   `@thing(icon: @doc.icon("x"))` -- describes `thing`, it is not part of what `thing` holds.
/// - the value of a `key: value` entry, for the same reason; only an element that is an entry of
///   `{value}` itself counts.
/// - a `:connect`: it is metadata about its owner, consumed only by the dedicated check that
///   understands the closed set of connect names. Feeding it through here would let `rule` in
///   `=[ h ]:rule(...)` reach the same classification a real element gets, which is the confusion
///   an element in a `:rule(...)` being reported as a disallowed *descendant* would be.
///
/// With `direct_only`, only the first level counts: an element is shown and not entered, a
/// paragraph is entered (its inline elements are direct), and nothing else is.
fn descendants<'a>(
    start: impl FnOnce(&mut dyn FnMut(Node<'a>, Place)),
    direct_only: bool,
    mut f: impl FnMut(Node<'a>),
) {
    let mut visit = |node: Node<'a>, place: Place| -> Visit<()> {
        if matches!(place, Place::Args | Place::PairValue | Place::Connect) {
            return Visit::Skip;
        }
        let is_target = matches!(node, Node::Item(_)) || node.element().is_some();
        if is_target {
            f(node);
            if direct_only {
                Visit::Skip
            } else {
                Visit::Descend
            }
        } else if direct_only && !matches!(node, Node::Block(Block::Paragraph(_))) {
            Visit::Skip
        } else {
            Visit::Descend
        }
    };
    start(&mut |node, place| {
        let _ = walk_node(node, place, &mut visit);
    });
}

/// What is nested in `el`: its `[content]` and the elements that are entries of its `{value}`.
/// With `direct_only`, only the first level.
///
/// Neither `el.args` nor `el.connects` is looked at; see `descendants` for why.
pub fn for_each_descendant(el: &Element, direct_only: bool, f: impl FnMut(Node<'_>)) {
    descendants(
        |g| groups(&el.data, el.content.as_deref(), g),
        direct_only,
        f,
    );
}

/// [`for_each_descendant`] for a list item: what is nested in its content and in its `{value}`
/// group, not its `sublist` and not its `(args)` marker.
pub fn for_each_descendant_of_item(item: &ListItem, direct_only: bool, f: impl FnMut(Node<'_>)) {
    descendants(
        |g| groups(&item.data, Some(&item.content), g),
        direct_only,
        f,
    );
}

/// Every descendant reachable from `blocks`, in document order: [`for_each_descendant`]'s own
/// `content` handling for any block sequence (a [`Section`]'s `blocks` has the same shape), so a
/// heading's own `:rule(...)` can check what sits *under* the heading.
pub fn for_each_descendant_in_blocks(blocks: &[Block], direct_only: bool, f: impl FnMut(Node<'_>)) {
    descendants(
        |g| {
            for block in blocks {
                g(Node::Block(block), Place::Content);
            }
        },
        direct_only,
        f,
    );
}

/// [`for_each_descendant_in_blocks`] for a list's own items: what a list's own `:rule(allow:...)`
/// checks against. `direct_only` means just the items themselves, not what is inside each one.
pub fn for_each_item_descendant(items: &[ListItem], direct_only: bool, f: impl FnMut(Node<'_>)) {
    descendants(
        |g| {
            for item in items {
                g(Node::Item(item), Place::Item);
            }
        },
        direct_only,
        f,
    );
}

/// What a mutable walk hands over. A mutable walk cannot share the borrow with [`children`], so it
/// lists the children again, in the same order; `tests/walk_coverage.rs` holds the two to the
/// same reach.
trait Sink {
    fn element(&mut self, _el: &mut Element) {}
    fn id(&mut self, _id: &mut Option<Id>, _span: Span) {}
    /// Each inline, before it is looked into. The sink may replace it.
    fn inline(&mut self, _inline: &mut Inline) {}
    /// Each block, before it is looked into. The sink may replace it.
    fn block(&mut self, _block: &mut Block) {}
    /// Each value, before it is looked into. The sink may replace it.
    fn value(&mut self, _value: &mut Value) {}
}

struct MutWalk<'s, S: Sink> {
    sink: &'s mut S,
    /// Whether to go into `:connect`s.
    connects: bool,
}

impl<S: Sink> MutWalk<'_, S> {
    fn declaration(&mut self, d: &mut Declaration) {
        self.sink.id(&mut d.id, d.span);
        self.groups(&mut d.data, d.content.as_deref_mut());
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    fn block(&mut self, block: &mut Block) {
        self.sink.block(block);
        match block {
            Block::Paragraph(p) => self.inlines(&mut p.content),
            Block::Element(el) => self.element(el),
            Block::Hole(h) => self.groups(&mut h.data, h.content.as_deref_mut()),
            Block::Section(s) => {
                self.sink.id(&mut s.id, s.span);
                self.inlines(&mut s.title);
                self.data(&mut s.data);
                for b in &mut s.blocks {
                    self.block(b);
                }
                self.connects(&mut s.connects);
            }
            Block::List(l) => self.list(l),
            Block::Interp(_) => {}
        }
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    fn inlines(&mut self, items: &mut [Inline]) {
        for inline in items {
            self.sink.inline(inline);
            match inline {
                Inline::Element(el) => self.element(el),
                Inline::Hole(h) => self.groups(&mut h.data, h.content.as_deref_mut()),
                Inline::Text(_)
                | Inline::SoftBreak(_)
                | Inline::LineBreak(_)
                | Inline::Raw(_)
                | Inline::Interp(_) => {}
            }
        }
    }

    fn list(&mut self, l: &mut List) {
        self.sink.id(&mut l.id, l.span);
        for item in &mut l.items {
            self.sink.id(&mut item.id, item.span);
            self.groups(&mut item.data, Some(&mut item.content));
            if let Some(sub) = &mut item.sublist {
                self.list(sub);
            }
            self.connects(&mut item.connects);
        }
        self.connects(&mut l.connects);
    }

    fn element(&mut self, el: &mut Element) {
        self.sink.element(el);
        self.sink.id(&mut el.id, el.span);
        self.groups(&mut el.data, el.content.as_deref_mut());
        self.connects(&mut el.connects);
    }

    fn connects(&mut self, items: &mut [Connect]) {
        if self.connects {
            for connect in items {
                self.groups(&mut connect.data, connect.content.as_deref_mut());
            }
        }
    }

    fn groups(&mut self, d: &mut Data, content: Option<&mut [Block]>) {
        self.data(d);
        for block in content.into_iter().flatten() {
            self.block(block);
        }
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    fn data(&mut self, d: &mut Data) {
        if let Some(args) = &mut d.args {
            self.value(args);
        }
        if let Some(Group(entries)) = &mut d.value {
            for entry in entries {
                match entry {
                    Entry::Pair(_, v) => self.value(v),
                    Entry::Element(el) => self.element(el),
                    Entry::Hole(h) => self.groups(&mut h.data, h.content.as_deref_mut()),
                    Entry::Bare(b) => {
                        self.sink.id(&mut b.id, b.span);
                        self.groups(&mut b.data, b.content.as_deref_mut());
                    }
                }
            }
        }
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    fn value(&mut self, value: &mut Value) {
        self.sink.value(value);
        match value {
            Value::Element(el) => self.element(el),
            Value::Hole(h) => self.groups(&mut h.data, h.content.as_deref_mut()),
            Value::Seq(items) | Value::Call(_, items) => {
                for v in items {
                    self.value(v);
                }
            }
            Value::Map(entries) => {
                for (_, v) in entries {
                    self.value(v);
                }
            }
            Value::Blocks(blocks) => {
                for b in blocks {
                    self.block(b);
                }
            }
            Value::Null
            | Value::Interp(_)
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::Number(_)
            | Value::String(_) => {}
        }
    }
}

fn walk_mut(file: &mut File, sink: &mut impl Sink, connects: bool) {
    let mut walk = MutWalk { sink, connects };
    for decl in &mut file.header {
        walk.declaration(decl);
    }
    for block in &mut file.body {
        walk.block(block);
    }
}

/// Traverses all elements in `file` mutably in document order and executes `f` for each in place.
/// It reaches what [`for_each_element`] reaches, in the same order: an element in a `:connect` is
/// not reached.
pub fn for_each_element_mut(file: &mut File, f: impl FnMut(&mut Element)) {
    struct Elements<F>(F);
    impl<F: FnMut(&mut Element)> Sink for Elements<F> {
        fn element(&mut self, el: &mut Element) {
            (self.0)(el);
        }
    }
    walk_mut(file, &mut Elements(f), false);
}

/// Traverses all declarations in `file` mutably in document order and executes `f` for each in place.
pub fn for_each_declaration_mut(file: &mut File, mut f: impl FnMut(&mut Declaration)) {
    for decl in &mut file.header {
        f(decl);
    }
}

/// Hands `f` the `#(id)` of every node of `file` that can carry one (an element, a declaration, a
/// section, a list, a list item, a bare entry), with the node's span, in document order, each node
/// before what it holds. It goes into `:connect`s too, as [`for_each_node`] does; the nodes it
/// reaches are those for which [`Node::id`] answers.
pub fn for_each_id_mut(file: &mut File, f: impl FnMut(&mut Option<Id>, Span)) {
    struct Ids<F>(F);
    impl<F: FnMut(&mut Option<Id>, Span)> Sink for Ids<F> {
        fn id(&mut self, id: &mut Option<Id>, span: Span) {
            (self.0)(id, span);
        }
    }
    walk_mut(file, &mut Ids(f), true);
}

/// Visits every [`Interpolation`] under `value`, in document order, wherever it sits: the value
/// itself, an element's `(args)` and `{value}` entries, running text and blocks of a `[...]`.
///
/// Unlike [`for_each_descendant_in_blocks`] this goes into `(args)` and the values of
/// `key: value` entries, because here the question is "what is written anywhere in this value", and
/// not "what does this element hold". `:connect`s are reached, as [`for_each_node`] reaches them.
pub fn for_each_interpolation_in_value(value: &Value, mut f: impl FnMut(&Interpolation)) {
    let _ = walk_node(Node::Value(value), Place::Part, &mut |node, _| {
        if let Some(interp) = node.interpolation() {
            f(interp);
        }
        Visit::<()>::Descend
    });
}

/// Replaces, in place, each interpolation under `value` that `fill` has a value for. It is shown
/// them in the order [`for_each_interpolation_in_value`] shows them, and an interpolation it answers
/// `None` for is left as written.
///
/// What the value becomes depends on where the interpolation sits, which is the one thing this
/// function knows:
///
/// - as a value (`(args)`, an entry of `{value}`, a `[...]` value): the value itself, whatever type
///   it has;
/// - in running text: text, of the value's scalar text;
/// - as a block of its own: a paragraph of that text.
///
/// A value with no scalar text (a map, a list) has no place in text, so there the interpolation is
/// left as written too.
pub fn fill_interpolations(value: &mut Value, fill: impl FnMut(&Interpolation) -> Option<Value>) {
    struct Fill<F>(F);
    impl<F: FnMut(&Interpolation) -> Option<Value>> Fill<F> {
        fn text(&mut self, interp: &Interpolation) -> Option<Text> {
            let filled = (self.0)(interp)?;
            let text = filled.scalar_text()?.into_owned();
            Some(Text::new(text, interp.span))
        }
    }
    impl<F: FnMut(&Interpolation) -> Option<Value>> Sink for Fill<F> {
        fn inline(&mut self, inline: &mut Inline) {
            if let Inline::Interp(interp) = inline
                && let Some(text) = self.text(interp)
            {
                *inline = Inline::Text(text);
            }
        }
        fn block(&mut self, block: &mut Block) {
            if let Block::Interp(interp) = block
                && let Some(text) = self.text(interp)
            {
                let span = interp.span;
                *block = Block::Paragraph(Paragraph::new(vec![Inline::Text(text)], span));
            }
        }
        fn value(&mut self, value: &mut Value) {
            if let Value::Interp(interp) = value
                && let Some(filled) = (self.0)(interp)
            {
                *value = filled;
            }
        }
    }
    MutWalk {
        sink: &mut Fill(fill),
        connects: true,
    }
    .value(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Block;

    fn element_of(file: &File) -> &Element {
        match &file.body[0] {
            Block::Element(el) => el,
            other => panic!("expected an element, got {other:?}"),
        }
    }

    fn names(el: &Element, direct_only: bool) -> Vec<String> {
        let mut out = Vec::new();
        for_each_descendant(el, direct_only, |node| {
            out.push(
                node.element_view()
                    .map(|v| v.name.name.clone())
                    .unwrap_or_default(),
            );
        });
        out
    }

    #[test]
    fn direct_only_stops_at_the_first_level() {
        let file = tomet_ast_parser::parse_file("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
        let outer = element_of(&file);
        assert_eq!(names(outer, true), vec!["mid"]);
    }

    #[test]
    fn every_descendant_walks_every_depth() {
        let file = tomet_ast_parser::parse_file("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
        let outer = element_of(&file);
        assert_eq!(names(outer, false), vec!["mid", "inner"]);
    }

    #[test]
    fn descendant_walk_never_visits_the_element_itself() {
        let file = tomet_ast_parser::parse_file("@outer[ @mid[ text ] ]\n").unwrap();
        let outer = element_of(&file);
        assert!(!names(outer, false).contains(&"outer".to_string()));
    }

    /// `<` for entering and `>` for leaving, `P` for a paragraph and `E` for an element.
    fn scope_events(src: &str, enter_answer: Visit) -> String {
        let file = tomet_ast_parser::parse_file(src).unwrap();
        let events = std::cell::RefCell::new(String::new());
        let label = |node: Node<'_>| match node {
            Node::Block(Block::Paragraph(_)) => Some('P'),
            Node::Block(Block::Element(_)) | Node::Inline(Inline::Element(_)) => Some('E'),
            _ => None,
        };
        for block in &file.body {
            let _ = walk_node_scoped(
                Node::Block(block),
                Place::Top,
                &mut |node, _| {
                    if let Some(c) = label(node) {
                        let mut events = events.borrow_mut();
                        events.push('<');
                        events.push(c);
                        if c == 'E' {
                            return enter_answer;
                        }
                    }
                    Visit::Descend
                },
                &mut |node, _| {
                    if let Some(c) = label(node) {
                        let mut events = events.borrow_mut();
                        events.push('>');
                        events.push(c);
                    }
                },
            );
        }
        events.into_inner()
    }

    #[test]
    fn a_scoped_walk_leaves_a_node_after_everything_it_holds() {
        // The paragraph inside the inline element is told apart from the two around it by
        // when it is left, which is what a walker that only sees entries cannot do.
        assert_eq!(
            scope_events("a @em[ x ] b\n", Visit::Descend),
            "<P<E<P>P>E>P"
        );
    }

    #[test]
    fn a_scoped_walk_leaves_a_node_it_was_told_to_skip() {
        assert_eq!(scope_events("a @em[ x ] b\n", Visit::Skip), "<P<E>E>P");
    }

    /// The `[...]` value of `::macros(m: [ ... ])` in `src`.
    fn macro_body(file: &mut File) -> &mut Value {
        let Some(Value::Map(entries)) = &mut file.header[0].data.args else {
            panic!("expected `(name: template)` entries");
        };
        &mut entries[0].1
    }

    /// Two blocks: an element on a line of its own, then a `${2}` on a line of its own.
    const BODY: &str = "::macros(m: [\n@card(title: ${1}, n: ${3})[ hi ${name} ]\n\n${2}\n])\n";

    fn interpolations(value: &Value) -> Vec<String> {
        let mut found = Vec::new();
        for_each_interpolation_in_value(value, |i| found.push(i.expr.to_string()));
        found
    }

    #[test]
    fn the_interpolations_under_a_value_include_arguments_and_entries() {
        let mut file = tomet_ast_parser::parse_file(BODY).unwrap();
        // `(args)` is where a descendant walk does not look; here it is the point.
        assert_eq!(
            interpolations(macro_body(&mut file)),
            ["1", "3", "name", "2"]
        );
    }

    #[test]
    fn a_filled_interpolation_is_a_value_in_data_and_text_in_text() {
        let mut file = tomet_ast_parser::parse_file(BODY).unwrap();
        let body = macro_body(&mut file);
        fill_interpolations(body, |i| match i.expr.to_string().as_str() {
            "1" => Some(Value::String("Welcome".to_string())),
            "3" => Some(Value::Int(5)),
            "name" => Some(Value::Int(7)),
            "2" => Some(Value::String("end".to_string())),
            _ => None,
        });
        assert!(
            interpolations(body).is_empty(),
            "{:?}",
            interpolations(body)
        );
        let Value::Blocks(blocks) = &*body else {
            panic!("expected blocks");
        };
        let Block::Element(card) = &blocks[0] else {
            panic!("expected the card, got {:?}", blocks[0]);
        };
        let Some(Value::Map(args)) = &card.data.args else {
            panic!("expected named arguments, got {:?}", card.data.args);
        };
        assert_eq!(args[0].1, Value::String("Welcome".to_string()));
        assert_eq!(args[1].1, Value::Int(5), "a number stays a number in data");
        let Some([Block::Paragraph(p)]) = card.content.as_deref() else {
            panic!("expected one paragraph, got {:?}", card.content);
        };
        assert!(
            matches!(&p.content[..], [Inline::Text(a), Inline::Text(b)]
                if a.value == "hi " && b.value == "7"),
            "{:?}",
            p.content
        );
        let Block::Paragraph(last) = &blocks[1] else {
            panic!(
                "a block of its own becomes a paragraph, got {:?}",
                blocks[1]
            );
        };
        assert!(matches!(&last.content[..], [Inline::Text(t)] if t.value == "end"));
    }

    #[test]
    fn an_interpolation_nothing_fills_is_left_as_written() {
        let mut file = tomet_ast_parser::parse_file(BODY).unwrap();
        let body = macro_body(&mut file);
        fill_interpolations(body, |i| {
            (i.expr.to_string() == "2").then_some(Value::Int(1))
        });
        assert_eq!(interpolations(body), ["1", "3", "name"]);
    }

    #[test]
    fn a_value_that_has_no_text_has_no_place_in_text() {
        let mut file = tomet_ast_parser::parse_file(BODY).unwrap();
        let body = macro_body(&mut file);
        fill_interpolations(body, |_| Some(Value::Map(Vec::new())));
        // Data takes a map as it is; running text and a block of its own cannot.
        assert_eq!(interpolations(body), ["name", "2"]);
    }

    #[test]
    fn every_interpolation_is_found_wherever_it_sits() {
        let src = "=[ ${title} ]:rule(allow: ${conn})\n\
                   \n\
                   ${block}\n\
                   \n\
                   text ${inline} more\n\
                   \n\
                   @x(a: ${arg}, b: (${nested})){ k: ${val} }[ ${content} ]:as(${connect})\n\
                   \n\
                   - item ${item}\n";
        let file = tomet_ast_parser::parse_file(src).unwrap();
        let mut found = Vec::new();
        for_each_interpolation(&file, |i| found.push(i.expr.to_string()));
        found.sort();
        let mut expected = vec![
            "title", "conn", "block", "inline", "arg", "nested", "val", "content", "connect",
            "item",
        ];
        expected.sort();
        assert_eq!(found, expected);
    }
}
