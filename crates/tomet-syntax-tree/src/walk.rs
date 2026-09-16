//! Generic recursive traversal of a [`tomet_ast::Document`]'s tree.

use std::ops::ControlFlow;
use tomet_ast::{Block, Document, Element, ElementValue, Entry, Inline, Value};

/// Called at every [`Element`] `walk_document` visits, in document order.
pub trait Visitor<B> {
    fn visit(&mut self, el: &Element) -> ControlFlow<B>;
}

impl<F, B> Visitor<B> for F
where
    F: FnMut(&Element) -> ControlFlow<B>,
{
    fn visit(&mut self, el: &Element) -> ControlFlow<B> {
        self(el)
    }
}

/// Traverses all elements in `doc` in document order and executes `f` for each.
pub fn for_each_element(doc: &Document, mut f: impl FnMut(&Element)) {
    let mut visitor = |el: &Element| -> ControlFlow<()> {
        f(el);
        ControlFlow::Continue(())
    };
    let _ = walk_document(doc, &mut visitor);
}

/// [`for_each_element`] over one [`Block`], for callers that walk the
/// document's own top level themselves and need the elements below it.
pub fn for_each_element_in_block(block: &Block, mut f: impl FnMut(&Element)) {
    let mut visitor = |el: &Element| -> ControlFlow<()> {
        f(el);
        ControlFlow::Continue(())
    };
    let _ = walk_block(block, &mut visitor);
}

macro_rules! propagate {
    ($e:expr) => {
        match $e {
            ControlFlow::Continue(()) => {}
            broke @ ControlFlow::Break(_) => return broke,
        }
    };
}

/// Depth-first, document-order walk of every `Element` in `doc`.
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
    if let Some(args) = &element.args {
        propagate!(walk_value(args, visitor));
    }
    if let Some(content) = &element.content {
        propagate!(walk_inlines(content, visitor));
    }
    if let Some(children) = &element.children {
        for child in children {
            propagate!(walk_block(child, visitor));
        }
    }
    if let Some(ElementValue::Group(entries)) = &element.value {
        for entry in entries {
            match entry {
                Entry::Element(child) => propagate!(walk_element(child, visitor)),
                Entry::Pair(_, v) => propagate!(walk_value(v, visitor)),
            }
        }
    }
    ControlFlow::Continue(())
}

/// Descends into a [`Value`] for any [`Value::Element`] nested inside it --
/// directly, or under a [`Value::Seq`]/[`Value::Map`]/[`Value::Call`] that
/// contains one. Every other position an element can sit in (`[content]`,
/// `{value}`'s bare [`Entry::Element`], `children`) already had a walker
/// before `Value::Element` existed; this is the one `element.args` and
/// `Entry::Pair`'s value needed, since neither was ever visited at all
/// while `Value` had no element-shaped variant to visit.
fn walk_value<B>(value: &Value, visitor: &mut impl Visitor<B>) -> ControlFlow<B> {
    match value {
        Value::Element(el) => walk_element(el, visitor),
        Value::Seq(items) => {
            for item in items {
                propagate!(walk_value(item, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                propagate!(walk_value(v, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Call(_, args) => {
            for a in args {
                propagate!(walk_value(a, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_) => {
            ControlFlow::Continue(())
        }
    }
}

/// Mutable counterpart of [`Visitor`] -- called at every `&mut Element`
/// in document order.
pub trait VisitorMut<B> {
    fn visit_mut(&mut self, el: &mut Element) -> ControlFlow<B>;
}

impl<F, B> VisitorMut<B> for F
where
    F: FnMut(&mut Element) -> ControlFlow<B>,
{
    fn visit_mut(&mut self, el: &mut Element) -> ControlFlow<B> {
        self(el)
    }
}

/// Traverses all elements in `doc` mutably in document order and executes `f` for each in place.
pub fn for_each_element_mut(doc: &mut Document, mut f: impl FnMut(&mut Element)) {
    let mut visitor = |el: &mut Element| -> ControlFlow<()> {
        f(el);
        ControlFlow::Continue(())
    };
    let _ = walk_document_mut(doc, &mut visitor);
}

/// Mutable counterpart of [`walk_document`].
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
    if let Some(args) = &mut element.args {
        propagate!(walk_value_mut(args, visitor));
    }
    if let Some(content) = &mut element.content {
        propagate!(walk_inlines_mut(content, visitor));
    }
    if let Some(children) = &mut element.children {
        for child in children {
            propagate!(walk_block_mut(child, visitor));
        }
    }
    if let Some(ElementValue::Group(entries)) = &mut element.value {
        for entry in entries {
            match entry {
                Entry::Element(child) => propagate!(walk_element_mut(child, visitor)),
                Entry::Pair(_, v) => propagate!(walk_value_mut(v, visitor)),
            }
        }
    }
    ControlFlow::Continue(())
}

/// Mutable counterpart of [`walk_value`].
fn walk_value_mut<B>(value: &mut Value, visitor: &mut impl VisitorMut<B>) -> ControlFlow<B> {
    match value {
        Value::Element(el) => walk_element_mut(el, visitor),
        Value::Seq(items) => {
            for item in items {
                propagate!(walk_value_mut(item, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                propagate!(walk_value_mut(v, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Call(_, args) => {
            for a in args {
                propagate!(walk_value_mut(a, visitor));
            }
            ControlFlow::Continue(())
        }
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_) => {
            ControlFlow::Continue(())
        }
    }
}

/// Every descendant of `el` (not `el` itself), in document order --
/// `content`, `children`, and `ElementValue::Group` entries. `:rule`'s own
/// use is the only caller, so "descendant" means what `:rule(allow:...)`
/// means by it: content and children the reader encounters *underneath*
/// `el`, not configuration carried *on* `el`.
///
/// That is also why this deliberately never looks at `el.args`, even
/// though [`walk_element`] does (`args` can hold a [`Value::Element`]
/// since elements were allowed to sit in value position): an element
/// embedded in `el`'s own `(args)` -- `@thing(icon: @doc.icon("x"))` --
/// describes `thing`, the same way `el.connects` does, so it is exempt
/// for the same reason. `el.connects` itself is skipped for a related but
/// distinct reason: a connect (`Sigil::Named` carrying `:name(...)`'s own
/// args/content/value, e.g. `:rule(...)`) is not a descendant of the
/// document `el` sits in at all -- it is metadata about `el` itself,
/// consumed only by the dedicated check that understands the closed set
/// of connect names. Feeding either through here would let it reach the
/// same classification a real document element gets, which is exactly the
/// confusion a `:rule(...)` element (or an `@doc.icon` describing `thing`)
/// being reported as a disallowed *descendant* would be.
///
/// `direct_only` restricts to immediate children only -- one level of
/// `content`/`children`/`ElementValue::Group`, not recursed further --
/// for `:rule(direct:true)`; `false` walks every descendant at any
/// depth, same as [`walk_element`].
pub fn for_each_descendant(el: &Element, direct_only: bool, mut f: impl FnMut(&Element)) {
    if direct_only {
        if let Some(content) = &el.content {
            for inline in content {
                if let Inline::Element(child) = inline {
                    f(child);
                }
            }
        }
        if let Some(children) = &el.children {
            for block in children {
                if let Block::Element(child) = block {
                    f(child);
                }
            }
        }
        if let Some(ElementValue::Group(entries)) = &el.value {
            for entry in entries {
                if let Entry::Element(child) = entry {
                    f(child);
                }
            }
        }
        return;
    }
    let mut visitor = |child: &Element| -> ControlFlow<()> {
        f(child);
        ControlFlow::Continue(())
    };
    if let Some(content) = &el.content {
        let _ = walk_inlines(content, &mut visitor);
    }
    if let Some(children) = &el.children {
        for block in children {
            let _ = walk_block(block, &mut visitor);
        }
    }
    if let Some(ElementValue::Group(entries)) = &el.value {
        for entry in entries {
            if let Entry::Element(child) = entry {
                let _ = walk_element(child, &mut visitor);
            }
        }
    }
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
        if filter(el)
            && transform(el) {
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
            out.push(
                child
                    .sigil
                    .name()
                    .map(|n| n.name.clone())
                    .unwrap_or_default(),
            );
        });
        out
    }

    #[test]
    fn direct_only_stops_at_the_first_level() {
        let doc = tomet_parser::parse_document("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert_eq!(names(outer, true), vec!["mid"]);
    }

    #[test]
    fn every_descendant_walks_every_depth() {
        let doc = tomet_parser::parse_document("@outer[ @mid[ @inner[ text ] ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert_eq!(names(outer, false), vec!["mid", "inner"]);
    }

    #[test]
    fn descendant_walk_never_visits_the_element_itself() {
        let doc = tomet_parser::parse_document("@outer[ @mid[ text ] ]\n").unwrap();
        let outer = element_of(&doc);
        assert!(!names(outer, false).contains(&"outer".to_string()));
    }
}
