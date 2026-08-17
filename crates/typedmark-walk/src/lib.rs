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
    walk_inlines(&item.content, visitor)
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
}
