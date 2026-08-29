//! Generic recursive traversal of a `tomet_ast::Document`'s tree.
//! Exists so consumers that need to visit every id-bearing node
//! (`tomet-validator`'s duplicate-id check, `tomet-resolver`'s
//! `${...}` id lookup, and future ones) don't each hand-roll their own
//! copy of "how do I recurse through `Block`/`Inline`/
//! `ElementValue::Children`/`Element.children`" -- that shape is tied to
//! `tomet-ast`'s node definitions, not to any one consumer's business
//! logic, so it lives here instead: a new crate depending on nothing but
//! `tomet-ast`, the same role `tomet-semantics` plays for
//! "what does this element mean" (see `docs/develop/architecture.md`).
//! Deliberately not part of `tomet-ast` itself -- that crate is pure
//! type definitions with no traversal logic of its own, and deliberately
//! not part of `tomet-resolver` -- validator explicitly does not want
//! resolve's I/O-and-`tomet-parser` baggage as a transitive
//! dependency just to reuse a tree walk.
//!
//! Every visited node is a plain `Element` -- a list item is just
//! `Element{ sigil: Sigil::Bare, .. }` (see `tomet_ast::Element::list_item`),
//! so there's no separate id-bearing node kind to special-case here
//! anymore.

use std::ops::ControlFlow;
use tomet_ast::{Block, Document, Element, ElementValue, Inline, Value};

/// Merges an [`Element`]'s `args` and `value` (when `value` is
/// `ElementValue::Data(Value::Map(_))`) into one attrs view -- `value`'s
/// keys win on conflict. Mirrors `tomet-doc-resolver::interp::node_value`,
/// which solves the same `${id}`/`${id.member}` merge problem; kept as a
/// separate copy here since this crate depends on nothing but
/// `tomet-ast`.
pub fn element_attrs_view(el: &Element) -> Option<Value> {
    let args_map = match &el.args {
        Some(Value::Map(entries)) => Some(entries.clone()),
        _ => None,
    };
    let value_map = match &el.value {
        Some(ElementValue::Data(Value::Map(entries))) => Some(entries.clone()),
        _ => None,
    };
    match (args_map, value_map) {
        (Some(mut merged), Some(value_entries)) => {
            for (key, value) in value_entries {
                match merged.iter_mut().find(|(k, _)| *k == key) {
                    Some(existing) => existing.1 = value,
                    None => merged.push((key, value)),
                }
            }
            Some(Value::Map(merged))
        }
        (Some(args), None) => Some(Value::Map(args)),
        (None, Some(value)) => Some(Value::Map(value)),
        (None, None) => match &el.value {
            Some(ElementValue::Data(v)) => Some(v.clone()),
            _ => el.args.clone(),
        },
    }
}

/// Mutable counterpart of [`element_attrs_view`] -- a writable attrs slot
/// so a [`VisitorMut`] can insert/rename/replace entries in place.
///
/// Deliberately **not** a mirror of `element_attrs_view`'s merge: a merged
/// view spanning two fields (`args`/`value`) can't be written back through
/// one `&mut Value`. Instead this reads/writes `el.args` primarily (as it
/// always has), falling back to `el.value`'s inner map only when `el.args`
/// is `None` and `el.value` is `Some(ElementValue::Data(Value::Map(_)))` --
/// covers a heading (whose `{id:x}` data lives in `value`, `args` holds
/// only `level`) without changing behavior for every other element, whose
/// writable attrs have always lived in `args`.
pub fn element_attrs_mut(el: &mut Element) -> Option<&mut Value> {
    if el.args.is_some() {
        el.args.as_mut()
    } else if matches!(&el.value, Some(ElementValue::Data(Value::Map(_)))) {
        match &mut el.value {
            Some(ElementValue::Data(v)) => Some(v),
            _ => unreachable!(),
        }
    } else {
        None
    }
}

/// Called at every [`Element`] `walk_document` visits, in document order.
/// Return `ControlFlow::Continue(())` to keep walking, or
/// `ControlFlow::Break(b)` to stop immediately -- `b` propagates all the
/// way back out of `walk_document` as its return value, so a "find the
/// first match" caller can carry its result out through `Break` (see
/// `tomet-resolver`'s id lookup) while a "collect everything" caller
/// just always returns `Continue` and reads back whatever it accumulated
/// on `self` (see `tomet-validator`'s duplicate-id check).
pub trait Visitor<B> {
    fn visit(&mut self, el: &Element) -> ControlFlow<B>;
}

macro_rules! propagate {
    ($e:expr) => {
        match $e {
            ControlFlow::Continue(()) => {}
            broke @ ControlFlow::Break(_) => return broke,
        }
    };
}

/// Depth-first, document-order walk of every `Element` in `doc` (a heading
/// or a list item included -- both are just `Element`s, see the module
/// doc), including ones nested inside an element's `[content]`,
/// `children`, and `ElementValue::Children`.
pub fn walk_document<B>(doc: &Document, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    for block in &doc.blocks {
        propagate!(walk_block(block, visitor));
    }
    ControlFlow::Continue(())
}

fn walk_block<B>(block: &Block, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    match block {
        Block::Paragraph(paragraph) => walk_inlines(&paragraph.content, visitor),
        Block::Element(element) => walk_element(element, visitor),
    }
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
    propagate!(visitor.visit(element));
    if let Some(content) = &element.content {
        propagate!(walk_inlines(content, visitor));
    }
    if let Some(children) = &element.children {
        for child in children {
            propagate!(walk_block(child, visitor));
        }
    }
    if let Some(ElementValue::Children(children)) = &element.value {
        for child in children {
            propagate!(walk_element(child, visitor));
        }
    }
    ControlFlow::Continue(())
}

/// Mutable counterpart of [`Visitor`] -- called at every `&mut Element`
/// `walk_document_mut` visits, in document order, with `&mut` access.
/// Same `Continue`/`Break` semantics as [`Visitor::visit`].
pub trait VisitorMut<B> {
    fn visit_mut(&mut self, el: &mut Element) -> ControlFlow<B>;
}

/// Mutable counterpart of [`walk_document`] -- same depth-first,
/// document-order shape (including a list item's `children`), but
/// rewriting nodes in place instead of only reading them.
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
        Block::Paragraph(paragraph) => walk_inlines_mut(&mut paragraph.content, visitor),
        Block::Element(element) => walk_element_mut(element, visitor),
    }
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
    propagate!(visitor.visit_mut(element));
    if let Some(content) = &mut element.content {
        propagate!(walk_inlines_mut(content, visitor));
    }
    if let Some(children) = &mut element.children {
        for child in children {
            propagate!(walk_block_mut(child, visitor));
        }
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
        tomet_parser::parse_document(src).expect("valid Tomet source")
    }

    struct Collect(Vec<String>);
    impl Visitor<()> for Collect {
        fn visit(&mut self, _el: &Element) -> ControlFlow<()> {
            self.0.push("element".to_string());
            ControlFlow::Continue(())
        }
    }

    #[test]
    fn visits_every_node_kind_in_document_order() {
        // A heading is `Element` (classified `heading` by
        // `tomet-semantics::classify`); a list item is `Element`
        // (`sigil: Bare`) too -- everything visited is one node kind.
        let doc = parse("#[ h ]\n\n- item\n\n<x>()\n");
        let mut collect = Collect(Vec::new());
        let _ = walk_document(&doc, &mut collect);
        assert_eq!(collect.0, vec!["element", "element", "element", "element"]);
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
            fn visit(&mut self, _el: &Element) -> ControlFlow<u32> {
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
        // Regression test for `element_attrs_view`'s args+value merge: a
        // heading's `{id:h1}` lives in `Element.value` (its `args` holds
        // only the bare `level` int), not `Element.args` -- without the
        // merge, `h1` would be invisible here.
        let doc = parse("#[ h ]{ id:h1 }\n\n- item {id:i1}\n\n<x>(id:e1)\n");
        let mut ids = Vec::new();
        struct Ids<'a>(&'a mut Vec<String>);
        impl<'a> Visitor<()> for Ids<'a> {
            fn visit(&mut self, el: &Element) -> ControlFlow<()> {
                if let Some(Value::Map(entries)) = element_attrs_view(el) {
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
        fn visit_mut(&mut self, _el: &mut Element) -> ControlFlow<()> {
            self.0.push("element".to_string());
            ControlFlow::Continue(())
        }
    }

    #[test]
    fn walk_document_mut_visits_every_node_kind_in_document_order() {
        let mut doc = parse("#[ h ]\n\n- item\n\n<x>()\n");
        let mut collect = CollectMut(Vec::new());
        let _ = walk_document_mut(&mut doc, &mut collect);
        assert_eq!(collect.0, vec!["element", "element", "element", "element"]);
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
        // `children` only ever holds a nested sub-list (deeper
        // indentation), never any other block directly -- see
        // `tomet-parser::list::parse_list_internal`.
        let doc = parse("- outer\n  - <x>()\n");
        let has_nested_block = matches!(
            &doc.blocks[0],
            Block::Element(list) if matches!(
                &list.value,
                Some(ElementValue::Children(items))
                    if items.first().is_some_and(|item| item.children.is_some())
            )
        );
        assert!(
            has_nested_block,
            "fixture doesn't actually nest a block under the list item: {doc:?}"
        );

        let mut doc = doc;
        let mut collect = CollectMut(Vec::new());
        let _ = walk_document_mut(&mut doc, &mut collect);
        // outer list, outer item, nested list, nested item, <x>()
        assert_eq!(
            collect.0,
            vec!["element", "element", "element", "element", "element"]
        );
    }

    #[test]
    fn walk_document_mut_break_stops_the_walk_immediately_and_propagates_the_value() {
        struct FindSecond {
            seen: u32,
        }
        impl VisitorMut<u32> for FindSecond {
            fn visit_mut(&mut self, _el: &mut Element) -> ControlFlow<u32> {
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
            fn visit_mut(&mut self, el: &mut Element) -> ControlFlow<()> {
                if let Some(Value::Map(entries)) = element_attrs_mut(el) {
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
            fn visit(&mut self, el: &Element) -> ControlFlow<()> {
                if let Some(Value::Map(entries)) = element_attrs_view(el) {
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
