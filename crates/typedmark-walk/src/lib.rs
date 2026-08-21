//! Generic recursive traversal of a `typedmark_ast::Document`'s tree.
//! Exists so consumers that need to visit every id-bearing node
//! (`typedmark-validator`'s duplicate-id check, `typedmark-resolve`'s
//! `${...}` id lookup, and future ones) don't each hand-roll their own
//! copy of "how do I recurse through `Block`/`Inline`/
//! `ElementValue::Children`" -- that shape is tied to `typedmark-ast`'s
//! node definitions, not to any one consumer's business logic, so it
//! lives here instead: a new crate depending on nothing but
//! `typedmark-ast`, the same role `typedmark-semantics` plays for
//! "what does this element mean" (see `docs/develop/architecture.md`).
//! Deliberately not part of `typedmark-ast` itself -- that crate is pure
//! type definitions with no traversal logic of its own, and deliberately
//! not part of `typedmark-resolve` -- validator explicitly does not want
//! resolve's I/O-and-`typedmark-parser` baggage as a transitive
//! dependency just to reuse a tree walk.

use std::ops::ControlFlow;
use typedmark_ast::{
    Block, Document, Element, ElementValue, Heading, Inline, ListItem, Span, Value,
};

/// A node that can carry an `{id:...}`/`(id:...)`-shaped attrs/args map:
/// a `Heading`, a `ListItem`, or an `Element` (whose "attrs" are its
/// `args`). Unifies the three id-bearing shapes so a [`Visitor`] only
/// needs one method instead of three near-identical ones.
pub enum Node<'a> {
    Heading(&'a Heading),
    ListItem(&'a ListItem),
    Element(&'a Element),
}

impl<'a> Node<'a> {
    /// The node's attrs/args map, if any -- `Heading.attrs`/
    /// `ListItem.attrs`/`Element.args`.
    pub fn attrs(&self) -> Option<&'a Value> {
        match self {
            Node::Heading(h) => h.attrs.as_ref(),
            Node::ListItem(item) => item.attrs.as_ref(),
            Node::Element(el) => el.args.as_ref(),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Node::Heading(h) => h.span,
            Node::ListItem(item) => item.span,
            Node::Element(el) => el.span,
        }
    }
}

/// Called at every [`Node`] `walk_document` visits, in document order.
/// Return `ControlFlow::Continue(())` to keep walking, or
/// `ControlFlow::Break(b)` to stop immediately -- `b` propagates all the
/// way back out of `walk_document` as its return value, so a "find the
/// first match" caller can carry its result out through `Break` (see
/// `typedmark-resolve`'s id lookup) while a "collect everything" caller
/// just always returns `Continue` and reads back whatever it accumulated
/// on `self` (see `typedmark-validator`'s duplicate-id check).
pub trait Visitor<B> {
    fn visit(&mut self, node: Node<'_>) -> ControlFlow<B>;
}

macro_rules! propagate {
    ($e:expr) => {
        match $e {
            ControlFlow::Continue(()) => {}
            broke @ ControlFlow::Break(_) => return broke,
        }
    };
}

/// Depth-first, document-order walk of every `Heading`/`ListItem`/
/// `Element` in `doc`, including ones nested inside an element's
/// `[content]` and `ElementValue::Children`.
pub fn walk_document<B>(doc: &Document, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    for block in &doc.blocks {
        propagate!(walk_block(block, visitor));
    }
    ControlFlow::Continue(())
}

fn walk_block<B>(block: &Block, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    match block {
        Block::Heading(heading) => walk_heading(heading, visitor),
        Block::Paragraph(paragraph) => walk_inlines(&paragraph.content, visitor),
        Block::List(list) => {
            for item in &list.items {
                propagate!(walk_list_item(item, visitor));
            }
            ControlFlow::Continue(())
        }
        Block::Element(element) => walk_element(element, visitor),
    }
}

fn walk_heading<B>(heading: &Heading, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    propagate!(visitor.visit(Node::Heading(heading)));
    walk_inlines(&heading.content, visitor)
}

fn walk_list_item<B>(item: &ListItem, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    propagate!(visitor.visit(Node::ListItem(item)));
    propagate!(walk_inlines(&item.content, visitor));
    for child in &item.children {
        propagate!(walk_block(child, visitor));
    }
    ControlFlow::Continue(())
}

fn walk_inlines<B>(inlines: &[Inline], visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    for inline in inlines {
        if let Inline::Element(element) = inline {
            propagate!(walk_element(element, visitor));
        }
    }
    ControlFlow::Continue(())
}

fn walk_element<B>(element: &Element, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    propagate!(visitor.visit(Node::Element(element)));
    if let Some(content) = &element.content {
        propagate!(walk_inlines(content, visitor));
    }
    if let Some(ElementValue::Children(children)) = &element.value {
        for child in children {
            propagate!(walk_element(child, visitor));
        }
    }
    ControlFlow::Continue(())
}

/// Mutable counterpart of [`Node`] -- same three id-bearing shapes, but
/// with `&mut` access so a [`VisitorMut`] can rewrite what it visits in
/// place.
pub enum NodeMut<'a> {
    Heading(&'a mut Heading),
    ListItem(&'a mut ListItem),
    Element(&'a mut Element),
}

impl<'a> NodeMut<'a> {
    /// The node's attrs/args map, if any -- mutable, so a visitor can
    /// insert/rename/replace entries in place. `Heading.attrs`/
    /// `ListItem.attrs`/`Element.args`.
    pub fn attrs_mut(&mut self) -> Option<&mut Value> {
        match self {
            NodeMut::Heading(h) => h.attrs.as_mut(),
            NodeMut::ListItem(item) => item.attrs.as_mut(),
            NodeMut::Element(el) => el.args.as_mut(),
        }
    }

    pub fn span(&self) -> Span {
        match self {
            NodeMut::Heading(h) => h.span,
            NodeMut::ListItem(item) => item.span,
            NodeMut::Element(el) => el.span,
        }
    }
}

/// Mutable counterpart of [`Visitor`] -- called at every [`NodeMut`]
/// `walk_document_mut` visits, in document order, with `&mut` access.
/// Same `Continue`/`Break` semantics as [`Visitor::visit`].
pub trait VisitorMut<B> {
    fn visit_mut(&mut self, node: NodeMut<'_>) -> ControlFlow<B>;
}

/// Mutable counterpart of [`walk_document`] -- same depth-first,
/// document-order shape (including `ListItem.children`), but rewriting
/// nodes in place instead of only reading them.
pub fn walk_document_mut<B>(
    doc: &mut Document,
    visitor: &mut impl VisitorMut<B>,
) -> ControlFlow<B> {
    for block in &mut doc.blocks {
        propagate!(walk_block_mut(block, visitor));
    }
    ControlFlow::Continue(())
}

fn walk_block_mut<B>(block: &mut Block, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    match block {
        Block::Heading(heading) => walk_heading_mut(heading, visitor),
        Block::Paragraph(paragraph) => walk_inlines_mut(&mut paragraph.content, visitor),
        Block::List(list) => {
            for item in &mut list.items {
                propagate!(walk_list_item_mut(item, visitor));
            }
            ControlFlow::Continue(())
        }
        Block::Element(element) => walk_element_mut(element, visitor),
    }
}

fn walk_heading_mut<B>(heading: &mut Heading, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    propagate!(visitor.visit_mut(NodeMut::Heading(heading)));
    walk_inlines_mut(&mut heading.content, visitor)
}

fn walk_list_item_mut<B>(item: &mut ListItem, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    propagate!(visitor.visit_mut(NodeMut::ListItem(item)));
    propagate!(walk_inlines_mut(&mut item.content, visitor));
    for child in &mut item.children {
        propagate!(walk_block_mut(child, visitor));
    }
    ControlFlow::Continue(())
}

fn walk_inlines_mut<B>(inlines: &mut [Inline], visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    for inline in inlines {
        if let Inline::Element(element) = inline {
            propagate!(walk_element_mut(element, visitor));
        }
    }
    ControlFlow::Continue(())
}

fn walk_element_mut<B>(element: &mut Element, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    propagate!(visitor.visit_mut(NodeMut::Element(element)));
    if let Some(content) = &mut element.content {
        propagate!(walk_inlines_mut(content, visitor));
    }
    if let Some(ElementValue::Children(children)) = &mut element.value {
        for child in children {
            propagate!(walk_element_mut(child, visitor));
        }
    }
    ControlFlow::Continue(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Document {
        typedmark_parser::parse_document(src).expect("valid TypedMark source")
    }

    struct Collect(Vec<String>);
    impl Visitor<()> for Collect {
        fn visit(&mut self, node: Node<'_>) -> ControlFlow<()> {
            self.0.push(
                match &node {
                    Node::Heading(_) => "heading",
                    Node::ListItem(_) => "list_item",
                    Node::Element(_) => "element",
                }
                .to_string(),
            );
            ControlFlow::Continue(())
        }
    }

    #[test]
    fn visits_every_node_kind_in_document_order() {
        let doc = parse("#[ h ]\n\n- item\n\n<x>()\n");
        let mut collect = Collect(Vec::new());
        let _ = walk_document(&doc, &mut collect);
        assert_eq!(collect.0, vec!["heading", "list_item", "element"]);
    }

    #[test]
    fn visits_elements_nested_in_content_and_children() {
        let doc = parse("<memo>[ @(id:inner) ]\n\n@links {\n  (a)[ note ]\n}\n");
        let mut collect = Collect(Vec::new());
        let _ = walk_document(&doc, &mut collect);
        // <memo> itself, the inline @(id:inner) inside its content,
        // @links itself, and the bare (a)[...] child inside it.
        assert_eq!(collect.0, vec!["element", "element", "element", "element"]);
    }

    #[test]
    fn break_stops_the_walk_immediately_and_propagates_the_value() {
        struct FindSecond {
            seen: u32,
        }
        impl Visitor<u32> for FindSecond {
            fn visit(&mut self, _node: Node<'_>) -> ControlFlow<u32> {
                self.seen += 1;
                if self.seen == 2 {
                    ControlFlow::Break(self.seen)
                } else {
                    ControlFlow::Continue(())
                }
            }
        }
        let doc = parse("<a>()\n\n<b>()\n\n<c>()\n");
        let result = walk_document(&doc, &mut FindSecond { seen: 0 });
        assert_eq!(result, ControlFlow::Break(2));
    }

    #[test]
    fn node_attrs_reads_the_right_field_per_kind() {
        let doc = parse("#[ h ]{ id:h1 }\n\n- item {id:i1}\n\n<x>(id:e1)\n");
        let mut ids = Vec::new();
        struct Ids<'a>(&'a mut Vec<String>);
        impl<'a> Visitor<()> for Ids<'a> {
            fn visit(&mut self, node: Node<'_>) -> ControlFlow<()> {
                if let Some(Value::Map(entries)) = node.attrs() {
                    for (k, v) in entries {
                        if k == "id" {
                            if let Value::String(s) = v {
                                self.0.push(s.clone());
                            }
                        }
                    }
                }
                ControlFlow::Continue(())
            }
        }
        let _ = walk_document(&doc, &mut Ids(&mut ids));
        assert_eq!(ids, vec!["h1", "i1", "e1"]);
    }

    struct CollectMut(Vec<String>);
    impl VisitorMut<()> for CollectMut {
        fn visit_mut(&mut self, node: NodeMut<'_>) -> ControlFlow<()> {
            self.0.push(
                match &node {
                    NodeMut::Heading(_) => "heading",
                    NodeMut::ListItem(_) => "list_item",
                    NodeMut::Element(_) => "element",
                }
                .to_string(),
            );
            ControlFlow::Continue(())
        }
    }

    #[test]
    fn walk_document_mut_visits_every_node_kind_in_document_order() {
        let mut doc = parse("#[ h ]\n\n- item\n\n<x>()\n");
        let mut collect = CollectMut(Vec::new());
        let _ = walk_document_mut(&mut doc, &mut collect);
        assert_eq!(collect.0, vec!["heading", "list_item", "element"]);
    }

    #[test]
    fn walk_document_mut_visits_elements_nested_in_content_and_children() {
        let mut doc = parse("<memo>[ @(id:inner) ]\n\n@links {\n  (a)[ note ]\n}\n");
        let mut collect = CollectMut(Vec::new());
        let _ = walk_document_mut(&mut doc, &mut collect);
        assert_eq!(collect.0, vec!["element", "element", "element", "element"]);
    }

    #[test]
    fn walk_document_mut_visits_blocks_nested_under_a_list_item() {
        // `children` only ever holds a nested sub-`List` (deeper
        // indentation), never an `Element` block directly -- see
        // `typedmark-parser::list::parse_list_internal`. So the nested
        // element here is itself the sub-item's inline content.
        let doc = parse("- outer\n  - <x>()\n");
        let has_nested_block = matches!(
            &doc.blocks[0],
            Block::List(list) if list.items.first().is_some_and(|i| !i.children.is_empty())
        );
        assert!(
            has_nested_block,
            "fixture doesn't actually nest a block under the list item: {doc:?}"
        );

        let mut doc = doc;
        let mut collect = CollectMut(Vec::new());
        let _ = walk_document_mut(&mut doc, &mut collect);
        assert_eq!(collect.0, vec!["list_item", "list_item", "element"]);
    }

    #[test]
    fn walk_document_mut_break_stops_the_walk_immediately_and_propagates_the_value() {
        struct FindSecond {
            seen: u32,
        }
        impl VisitorMut<u32> for FindSecond {
            fn visit_mut(&mut self, _node: NodeMut<'_>) -> ControlFlow<u32> {
                self.seen += 1;
                if self.seen == 2 {
                    ControlFlow::Break(self.seen)
                } else {
                    ControlFlow::Continue(())
                }
            }
        }
        let mut doc = parse("<a>()\n\n<b>()\n\n<c>()\n");
        let result = walk_document_mut(&mut doc, &mut FindSecond { seen: 0 });
        assert_eq!(result, ControlFlow::Break(2));
    }

    #[test]
    fn walk_document_mut_mutation_actually_sticks() {
        let mut doc = parse("<x>(id:e1)\n");
        struct Rename;
        impl VisitorMut<()> for Rename {
            fn visit_mut(&mut self, mut node: NodeMut<'_>) -> ControlFlow<()> {
                if let Some(Value::Map(entries)) = node.attrs_mut() {
                    for (k, v) in entries.iter_mut() {
                        if k == "id" {
                            *v = Value::String("renamed".to_string());
                        }
                    }
                }
                ControlFlow::Continue(())
            }
        }
        let _ = walk_document_mut(&mut doc, &mut Rename);

        let mut ids = Vec::new();
        struct Ids<'a>(&'a mut Vec<String>);
        impl<'a> Visitor<()> for Ids<'a> {
            fn visit(&mut self, node: Node<'_>) -> ControlFlow<()> {
                if let Some(Value::Map(entries)) = node.attrs() {
                    for (k, v) in entries {
                        if k == "id" {
                            if let Value::String(s) = v {
                                self.0.push(s.clone());
                            }
                        }
                    }
                }
                ControlFlow::Continue(())
            }
        }
        let _ = walk_document(&doc, &mut Ids(&mut ids));
        assert_eq!(ids, vec!["renamed"]);
    }
}
