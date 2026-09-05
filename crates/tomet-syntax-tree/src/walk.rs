//! Generic recursive traversal of a [`tomet_ast::Document`]'s tree.

use std::ops::ControlFlow;
use tomet_ast::{Block, Document, Element, ElementValue, Entry, Inline};

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
            if let Entry::Element(child) = entry {
                propagate!(walk_element(child, visitor));
            }
        }
    }
    ControlFlow::Continue(())
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
            if let Entry::Element(child) = entry {
                propagate!(walk_element_mut(child, visitor));
            }
        }
    }
    ControlFlow::Continue(())
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
        if filter(el) {
            if transform(el) {
                count += 1;
            }
        }
    });
    count
}
