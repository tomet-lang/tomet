//! The mutable walk: a sink for the things a walk may change.

use tomet_ast::{
    Block, Connect, Data, Declaration, Element, Entry, File, Group, Id, Inline, List, Value,
};
use tomet_span::Span;

/// What a mutable walk hands over. A mutable walk cannot share the borrow with [`children`], so it
/// lists the children again, in the same order; a test holds the two to the
/// same reach.
pub(super) trait Sink {
    fn element(&mut self, _el: &mut Element) {}
    fn id(&mut self, _id: &mut Option<Id>, _span: Span) {}
    /// Each inline, before it is looked into. The sink may replace it.
    fn inline(&mut self, _inline: &mut Inline) {}
    /// Each block, before it is looked into. The sink may replace it.
    fn block(&mut self, _block: &mut Block) {}
    /// Each value, before it is looked into. The sink may replace it.
    fn value(&mut self, _value: &mut Value) {}
}

pub(super) struct MutWalk<'s, S: Sink> {
    pub(super) sink: &'s mut S,
    /// Whether to go into `::connect`s.
    pub(super) connects: bool,
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
            Block::Interp(_) | Block::TypeSeat(_) => {}
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
                | Inline::Interp(_)
                | Inline::TypeSeat(_) => {}
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
            for v in args.values_mut() {
                self.value(v);
            }
        }
        if let Some(Group(entries)) = &mut d.value {
            for entry in entries {
                match entry {
                    Entry::Pair(_, v) => self.value(v),
                    Entry::Element(el) => self.element(el),
                    Entry::Hole(h) => self.groups(&mut h.data, h.content.as_deref_mut()),
                }
            }
        }
    }

    #[deny(clippy::wildcard_enum_match_arm)]
    pub(super) fn value(&mut self, value: &mut Value) {
        self.sink.value(value);
        match value {
            Value::Element(el) => self.element(el),
            Value::Hole(h) => self.groups(&mut h.data, h.content.as_deref_mut()),
            Value::Tuple(tuple) | Value::Call(_, tuple) => {
                for v in tuple.values_mut() {
                    self.value(v);
                }
            }
            Value::Blocks(blocks) => {
                for b in blocks {
                    self.block(b);
                }
            }
            Value::Scalar(_) | Value::Interp(_) | Value::TypeSeat(_) => {}
        }
    }
}

pub(super) fn walk_mut(file: &mut File, sink: &mut impl Sink, connects: bool) {
    let mut walk = MutWalk { sink, connects };
    for decl in &mut file.header {
        walk.declaration(decl);
    }
    for block in &mut file.body {
        walk.block(block);
    }
}

/// Traverses all elements in `file` mutably in document order and executes `f` for each in place.
/// It reaches what [`for_each_element`](crate::for_each_element) reaches, in the same order: an element in a `::connect` is
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
/// section, a list or a list item), with the node's span, in document order, each node
/// before what it holds. It goes into `::connect`s too, as [`for_each_node`](crate::for_each_node) does; the nodes it
/// reaches are those for which [`Node::id`](crate::Node::id) answers.
pub fn for_each_id_mut(file: &mut File, f: impl FnMut(&mut Option<Id>, Span)) {
    struct Ids<F>(F);
    impl<F: FnMut(&mut Option<Id>, Span)> Sink for Ids<F> {
        fn id(&mut self, id: &mut Option<Id>, span: Span) {
            (self.0)(id, span);
        }
    }
    walk_mut(file, &mut Ids(f), true);
}
