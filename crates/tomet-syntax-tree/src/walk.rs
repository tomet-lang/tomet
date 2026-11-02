//! One definition of what sits inside what in a [`Document`], and the walkers built on it.
//!
//! [`children`] says, for any [`Node`], which nodes sit directly inside it and in which [`Place`].
//! [`walk`] is the one recursion over that. Every other walker here is [`walk`] with a filter
//! and, where a position must stay unseen, a [`Visit::Skip`] written at the walker, so what a
//! walker leaves out is read in its own body and not in a copy of the recursion.
//!
//! The groups of a node come in one order: `(args)`, `{value}`, `[content]`, then its
//! `:connect`s. The AST keeps no order of groups, so this is the order of every walker.
//!
//! The one exception to "one recursion" is [`for_each_element_mut`]: a mutable walk cannot share
//! the borrow with [`children`], so it lists the children again, and `tests/walk_coverage.rs`
//! holds the two to the same reach.

use std::ops::ControlFlow;
use tomet_ast::{
    Block, Connect, Data, Declaration, Document, Element, Entry, Group, Hole, Id, Inline,
    Interpolation, List, ListItem, Section, Value,
};
use tomet_span::Span;

/// Where a node sits in the node that holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// A block of the document itself.
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
    /// An item of a `list(...)`, a map's value or a call's argument.
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

/// A thing in a document's tree, together with what it is in.
///
/// An element is not a node of its own: it is the `Block`, `Inline`, `Value` or `Entry` it stands
/// as (see [`Node::element`]). A list at the top of a document is a `Block`; the list nested in an
/// item is a [`Node::List`].
#[derive(Debug, Clone, Copy)]
pub enum Node<'a> {
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
            Node::Block(Block::Declaration(d)) => Some(d),
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
            Node::Block(Block::Declaration(d)) => d.id.as_ref(),
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
pub fn children<'a>(node: Node<'a>, f: &mut dyn FnMut(Node<'a>, Place)) {
    match node {
        Node::Block(block) => match block {
            Block::Paragraph(p) => inlines(&p.content, Place::Text, f),
            Block::Element(el) => element(el, f),
            Block::Declaration(d) => groups(&d.data, d.content.as_deref(), f),
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

/// Shows `f` every node of `doc`, each before what it holds, in document order, and goes into
/// what `f` answers [`Visit::Descend`] to. [`Visit::Break`] stops the walk and is handed back.
pub fn walk<'a, B>(
    doc: &'a Document,
    f: &mut impl FnMut(Node<'a>, Place) -> Visit<B>,
) -> ControlFlow<B> {
    for block in &doc.blocks {
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
    match f(node, place) {
        Visit::Break(b) => return ControlFlow::Break(b),
        Visit::Skip => return ControlFlow::Continue(()),
        Visit::Descend => {}
    }
    let mut flow = ControlFlow::Continue(());
    children(node, &mut |child, at| {
        if flow.is_continue() {
            flow = walk_node(child, at, f);
        }
    });
    flow
}

/// Visits every node of `doc` in document order, each before what it holds, wherever it sits:
/// running text, section titles, `:connect`s, and inside a [`Value`] (`Seq`, `Map`, `Call`, a
/// nested element, `[...]` blocks). It is the walk that skips nothing.
pub fn for_each_node(doc: &Document, mut f: impl FnMut(Node<'_>)) {
    let _ = walk(doc, &mut |node, _| {
        f(node);
        Visit::<()>::Descend
    });
}

/// Every node of `doc` that is part of what the document says: [`for_each_node`] without the
/// `:connect`s, which are metadata about their owner and not a part of the document (see
/// [`for_each_descendant`]). The element, list and section walkers are filters over this.
fn for_each_part(doc: &Document, mut f: impl FnMut(Node<'_>)) {
    let _ = walk(doc, &mut |node, place| {
        if place == Place::Connect {
            return Visit::<()>::Skip;
        }
        f(node);
        Visit::Descend
    });
}

/// Traverses all elements in `doc` in document order and executes `f` for each.
///
/// An element in a `:connect` is not reached.
pub fn for_each_element(doc: &Document, mut f: impl FnMut(&Element)) {
    for_each_part(doc, |node| {
        if let Some(el) = node.element() {
            f(el);
        }
    });
}

/// Traverses every [`Inline`] in `doc` in document order: paragraph content, section titles and
/// every `[content]`. An `Inline::Element` is itself passed to `f`, and then descended into.
pub fn for_each_inline(doc: &Document, mut f: impl FnMut(&Inline)) {
    for_each_part(doc, |node| {
        if let Node::Inline(inline) = node {
            f(inline);
        }
    });
}

/// Visits every [`ListItem`] in `doc`, in document order, at any depth: a list item is not an
/// [`Element`], so [`for_each_element`] never passes one.
pub fn for_each_list_item(doc: &Document, mut f: impl FnMut(&ListItem)) {
    for_each_part(doc, |node| {
        if let Node::Item(item) = node {
            f(item);
        }
    });
}

/// Visits every [`List`] in `doc`, in document order: those standing as a block, wherever the
/// block is, and the ones nested in a list item.
pub fn for_each_list(doc: &Document, mut f: impl FnMut(&List)) {
    for_each_part(doc, |node| {
        if let Some(list) = node.list() {
            f(list);
        }
    });
}

/// Visits every [`Section`] in `doc`, in document order, including ones nested under another
/// section's body or in an element's content: a heading's own `:name(...)` connect is checked
/// against its `blocks` ([`for_each_descendant_in_blocks`]) the way an element's connect is
/// checked against its own content.
pub fn for_each_section(doc: &Document, mut f: impl FnMut(&Section)) {
    for_each_part(doc, |node| {
        if let Some(section) = node.section() {
            f(section);
        }
    });
}

/// Visits every [`Interpolation`] in `doc`, in document order, wherever it sits.
///
/// An interpolation is not an [`Element`], so the element walkers never pass one to their
/// visitor; this is the walker to ask for "every `${...}` in the document".
pub fn for_each_interpolation(doc: &Document, mut f: impl FnMut(&Interpolation)) {
    for_each_node(doc, |node| {
        if let Some(interp) = node.interpolation() {
            f(interp);
        }
    });
}

/// Visits every [`Declaration`] in `doc`, in document order, wherever it sits: a declaration is a
/// [`Block`], and a block can be in a section's body or an element's content as well as at the
/// top, where only it belongs. [`for_each_top_level_declaration`] is the preamble.
pub fn for_each_declaration(doc: &Document, mut f: impl FnMut(&Declaration)) {
    for_each_node(doc, |node| {
        if let Some(declaration) = node.declaration() {
            f(declaration);
        }
    });
}

/// Visits every [`Hole`] in `doc`, in document order, wherever it sits: as a block, in running
/// text, as an entry of a `{...}` group or as a value.
pub fn for_each_hole(doc: &Document, mut f: impl FnMut(&Hole)) {
    for_each_node(doc, |node| {
        if let Some(hole) = node.hole() {
            f(hole);
        }
    });
}

/// Visits every element a reader would find by scanning only the top of the document, in
/// document order: every `Block::Element`, and every `Inline::Element` sitting directly in a
/// `Block::Paragraph`'s own content (an element that shares its paragraph with other text is
/// still at the top; something nested inside another element is not).
///
/// This is the one place that decision lives. A caller that wants "the elements at the top of
/// this document" should call this instead of walking `doc.blocks` by hand. Declarations are not
/// elements: see [`for_each_top_level_declaration`].
pub fn for_each_top_level_element(doc: &Document, mut f: impl FnMut(&Element)) {
    for block in &doc.blocks {
        match block {
            Block::Element(el) => f(el),
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        f(el);
                    }
                }
            }
            Block::Section(_)
            | Block::List(_)
            | Block::Declaration(_)
            | Block::Hole(_)
            | Block::Interp(_) => {}
        }
    }
}

/// Visits every declaration at the top level of `doc`, in order.
///
/// A declaration is always a [`Block`] of its own, so "the declarations at the top of this
/// document" is `doc.blocks` and nothing else: one in a section's body or an element's content is
/// misplaced, and the validator reports it (`OutsidePreamble`). A caller that wants the preamble
/// should call this instead of walking `doc.blocks` by hand.
pub fn for_each_top_level_declaration(doc: &Document, mut f: impl FnMut(&Declaration)) {
    for block in &doc.blocks {
        if let Block::Declaration(declaration) = block {
            f(declaration);
        }
    }
}

/// [`for_each_top_level_declaration`], mutably -- for a caller that rewrites a top-level
/// declaration in place (`::blueprint(x)` becoming `::kind(x)`, say) without changing how many
/// there are.
pub fn for_each_top_level_declaration_mut(doc: &mut Document, mut f: impl FnMut(&mut Declaration)) {
    for block in &mut doc.blocks {
        if let Block::Declaration(declaration) = block {
            f(declaration);
        }
    }
}

/// [`for_each_top_level_declaration`], removing every one `keep` rejects.
pub fn retain_top_level_declarations(
    doc: &mut Document,
    mut keep: impl FnMut(&Declaration) -> bool,
) {
    doc.blocks.retain(|block| match block {
        Block::Declaration(declaration) => keep(declaration),
        _ => true,
    });
}

/// Something nested in a node: an element, or a list item. What a `:rule(allow:...)` constrains,
/// and what the region rules run over.
#[derive(Debug, Clone, Copy)]
pub enum Child<'a> {
    Element(&'a Element),
    Item(&'a ListItem),
}

impl<'a> Child<'a> {
    /// Its name, if it has one: a plain list item has none.
    pub fn name(&self) -> Option<&'a tomet_ast::Name> {
        match self {
            Child::Element(el) => Some(&el.name),
            Child::Item(item) => item.name.as_ref(),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Child::Element(el) => el.span,
            Child::Item(item) => item.span,
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
    mut f: impl FnMut(Child<'a>),
) {
    let mut visit = |node: Node<'a>, place: Place| -> Visit<()> {
        if matches!(place, Place::Args | Place::PairValue | Place::Connect) {
            return Visit::Skip;
        }
        let child = match node {
            Node::Item(item) => Some(Child::Item(item)),
            other => other.element().map(Child::Element),
        };
        match child {
            Some(child) => {
                f(child);
                if direct_only {
                    Visit::Skip
                } else {
                    Visit::Descend
                }
            }
            None if direct_only && !matches!(node, Node::Block(Block::Paragraph(_))) => Visit::Skip,
            None => Visit::Descend,
        }
    };
    start(&mut |node, place| {
        let _ = walk_node(node, place, &mut visit);
    });
}

/// What is nested in `el`: its `[content]` and the elements that are entries of its `{value}`.
/// With `direct_only`, only the first level.
///
/// Neither `el.args` nor `el.connects` is looked at; see [`descendants`] for why.
pub fn for_each_descendant(el: &Element, direct_only: bool, f: impl FnMut(Child<'_>)) {
    descendants(
        |g| groups(&el.data, el.content.as_deref(), g),
        direct_only,
        f,
    );
}

/// [`for_each_descendant`] for a list item: what is nested in its content and in its `{value}`
/// group, not its `sublist` and not its `(args)` marker.
pub fn for_each_descendant_of_item(item: &ListItem, direct_only: bool, f: impl FnMut(Child<'_>)) {
    descendants(
        |g| groups(&item.data, Some(&item.content), g),
        direct_only,
        f,
    );
}

/// Every descendant reachable from `blocks`, in document order: [`for_each_descendant`]'s own
/// `content` handling for any block sequence (a [`Section`]'s `blocks` has the same shape), so a
/// heading's own `:rule(...)` can check what sits *under* the heading.
pub fn for_each_descendant_in_blocks(
    blocks: &[Block],
    direct_only: bool,
    f: impl FnMut(Child<'_>),
) {
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
pub fn for_each_item_descendant(items: &[ListItem], direct_only: bool, f: impl FnMut(Child<'_>)) {
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
}

struct MutWalk<'s, S: Sink> {
    sink: &'s mut S,
    /// Whether to go into `:connect`s.
    connects: bool,
}

impl<S: Sink> MutWalk<'_, S> {
    fn block(&mut self, block: &mut Block) {
        match block {
            Block::Paragraph(p) => self.inlines(&mut p.content),
            Block::Element(el) => self.element(el),
            Block::Declaration(d) => {
                self.sink.id(&mut d.id, d.span);
                self.groups(&mut d.data, d.content.as_deref_mut());
            }
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

    fn inlines(&mut self, items: &mut [Inline]) {
        for inline in items {
            match inline {
                Inline::Element(el) => self.element(el),
                Inline::Hole(h) => self.groups(&mut h.data, h.content.as_deref_mut()),
                _ => {}
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

    fn value(&mut self, value: &mut Value) {
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

fn walk_mut(doc: &mut Document, sink: &mut impl Sink, connects: bool) {
    let mut walk = MutWalk { sink, connects };
    for block in &mut doc.blocks {
        walk.block(block);
    }
}

/// Traverses all elements in `doc` mutably in document order and executes `f` for each in place.
/// It reaches what [`for_each_element`] reaches, in the same order: an element in a `:connect` is
/// not reached.
pub fn for_each_element_mut(doc: &mut Document, f: impl FnMut(&mut Element)) {
    struct Elements<F>(F);
    impl<F: FnMut(&mut Element)> Sink for Elements<F> {
        fn element(&mut self, el: &mut Element) {
            (self.0)(el);
        }
    }
    walk_mut(doc, &mut Elements(f), false);
}

/// Hands `f` the `#(id)` of every node of `doc` that can carry one (an element, a declaration, a
/// section, a list, a list item, a bare entry), with the node's span, in document order, each node
/// before what it holds. It goes into `:connect`s too, as [`for_each_node`] does; the nodes it
/// reaches are those for which [`Node::id`] answers.
pub fn for_each_id_mut(doc: &mut Document, f: impl FnMut(&mut Option<Id>, Span)) {
    struct Ids<F>(F);
    impl<F: FnMut(&mut Option<Id>, Span)> Sink for Ids<F> {
        fn id(&mut self, id: &mut Option<Id>, span: Span) {
            (self.0)(id, span);
        }
    }
    walk_mut(doc, &mut Ids(f), true);
}

/// Mutates all elements in `doc` matching `filter` by applying `transform`.
/// Returns the number of elements transformed.
pub fn transform_elements<F, T>(doc: &mut Document, mut filter: F, mut transform: T) -> usize
where
    F: FnMut(&Element) -> bool,
    T: FnMut(&mut Element) -> bool,
{
    let mut count = 0;
    for_each_element_mut(doc, |el| {
        if filter(el) && transform(el) {
            count += 1;
        }
    });
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Block;

    fn element_of(doc: &Document) -> &Element {
        match &doc.blocks[0] {
            Block::Element(el) => el,
            other => panic!("expected an element, got {other:?}"),
        }
    }

    fn names(el: &Element, direct_only: bool) -> Vec<String> {
        let mut out = Vec::new();
        for_each_descendant(el, direct_only, |child| {
            out.push(child.name().map(|n| n.name.clone()).unwrap_or_default());
        });
        out
    }

    #[test]
    fn direct_only_stops_at_the_first_level() {
        let doc = tomet_ast_parser::parse_document("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert_eq!(names(outer, true), vec!["mid"]);
    }

    #[test]
    fn every_descendant_walks_every_depth() {
        let doc = tomet_ast_parser::parse_document("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert_eq!(names(outer, false), vec!["mid", "inner"]);
    }

    #[test]
    fn descendant_walk_never_visits_the_element_itself() {
        let doc = tomet_ast_parser::parse_document("@outer[ @mid[ text ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert!(!names(outer, false).contains(&"outer".to_string()));
    }

    #[test]
    fn every_interpolation_is_found_wherever_it_sits() {
        let src = "=[ ${title} ]:rule(allow: ${conn})\n\
                   \n\
                   ${block}\n\
                   \n\
                   text ${inline} more\n\
                   \n\
                   @x(a: ${arg}, b: list(${nested})){ k: ${val} }[ ${content} ]:as(${connect})\n\
                   \n\
                   - item ${item}\n";
        let doc = tomet_ast_parser::parse_document(src).unwrap();
        let mut found = Vec::new();
        for_each_interpolation(&doc, |i| found.push(i.expr.to_string()));
        found.sort();
        let mut expected = vec![
            "title", "conn", "block", "inline", "arg", "nested", "val", "content", "connect",
            "item",
        ];
        expected.sort();
        assert_eq!(found, expected);
    }
}
