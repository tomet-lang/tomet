//! The walkers that are [`walk`] with a filter: one `for_each_*` for each thing a file holds.

use super::node::{Node, Place, Visit, groups, walk, walk_node};
use tomet_ast::{
    Block, Declaration, Element, File, Hole, Inline, Interpolation, List, ListItem, Section,
    TypeSeat, Value,
};

/// Visits every node of `file` in document order, each before what it holds, wherever it sits:
/// running text, section titles, `::connect`s, and inside a [`Value`] (a tuple, a call, a
/// nested element, `[...]` blocks). It is the walk that skips nothing.
pub fn for_each_node(file: &File, mut f: impl FnMut(Node<'_>)) {
    let _ = walk(file, &mut |node, _| {
        f(node);
        Visit::<()>::Descend
    });
}

/// Every node of `file` that is part of what the document says: [`for_each_node`] without the
/// `::connect`s, which are metadata about their owner and not a part of the document (see
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
/// An element in a `::connect` is not reached.
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
/// section's body or in an element's content: a heading's own `::name(...)` connect is checked
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
/// visitor; this is the walker to ask for "every `{$ ... }` in the document".
pub fn for_each_interpolation(file: &File, mut f: impl FnMut(&Interpolation)) {
    for_each_node(file, |node| {
        if let Some(interp) = node.interpolation() {
            f(interp);
        }
    });
}

/// Visits every [`TypeSeat`] in `file`, in document order, wherever it sits: as a block, in
/// running text or as a value.
///
/// A type seat is not an [`Element`] either, so the element walkers never pass one to their
/// visitor.
pub fn for_each_type_seat(file: &File, mut f: impl FnMut(&TypeSeat)) {
    for_each_node(file, |node| {
        if let Some(seat) = node.type_seat() {
            f(seat);
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
            Block::Section(_)
            | Block::List(_)
            | Block::Hole(_)
            | Block::Interp(_)
            | Block::TypeSeat(_) => {}
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
/// - a `::connect`: it is metadata about its owner, consumed only by the dedicated check that
///   understands the closed set of connect names. Feeding it through here would let `rule` in
///   `=[ h ]::rule(...)` reach the same classification a real element gets, which is the confusion
///   an element in a `::rule(...)` being reported as a disallowed *descendant* would be.
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
/// heading's own `::rule(...)` can check what sits *under* the heading.
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

/// [`for_each_descendant_in_blocks`] for a list's own items: what a list's own `::rule(allow:...)`
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

/// Visits every [`Interpolation`] under `value`, in document order, wherever it sits: the value
/// itself, an element's `(args)` and `{value}` entries, running text and blocks of a `[...]`.
///
/// Unlike [`for_each_descendant_in_blocks`] this goes into `(args)` and the values of
/// `key: value` entries, because here the question is "what is written anywhere in this value", and
/// not "what does this element hold". `::connect`s are reached, as [`for_each_node`] reaches them.
pub fn for_each_interpolation_in_value(value: &Value, mut f: impl FnMut(&Interpolation)) {
    let _ = walk_node(Node::Value(value), Place::Part, &mut |node, _| {
        if let Some(interp) = node.interpolation() {
            f(interp);
        }
        Visit::<()>::Descend
    });
}

/// [`for_each_interpolation_in_value`] for [`TypeSeat`]s: every one under `value`, in document
/// order, wherever it sits, `(args)`, the values of `key: value` entries and `::connect`s included.
pub fn for_each_type_seat_in_value(value: &Value, mut f: impl FnMut(&TypeSeat)) {
    let _ = walk_node(Node::Value(value), Place::Part, &mut |node, _| {
        if let Some(seat) = node.type_seat() {
            f(seat);
        }
        Visit::<()>::Descend
    });
}

/// Visits every [`Value::Call`] in `file`, in document order, wherever it sits.
pub fn for_each_call(file: &File, mut f: impl FnMut(&str, &tomet_ast::Tuple, tomet_span::Span)) {
    for_each_node(file, |node| {
        if let Some((head, args, span)) = node.call() {
            f(head, args, span);
        }
    });
}

/// Visits every [`Value::Call`] under `value`, in document order, wherever it sits.
pub fn for_each_call_in_value(
    value: &Value,
    mut f: impl FnMut(&str, &tomet_ast::Tuple, tomet_span::Span),
) {
    let _ = walk_node(Node::Value(value), Place::Part, &mut |node, _| {
        if let Some((head, args, span)) = node.call() {
            f(head, args, span);
        }
        Visit::<()>::Descend
    });
}

/// Visits every [`Value::Call`] under `declaration`, in document order, wherever it sits.
pub fn for_each_call_in_declaration(
    declaration: &Declaration,
    mut f: impl FnMut(&str, &tomet_ast::Tuple, tomet_span::Span),
) {
    let _ = walk_node(
        Node::Declaration(declaration),
        Place::Header,
        &mut |node, _| {
            if let Some((head, args, span)) = node.call() {
                f(head, args, span);
            }
            Visit::<()>::Descend
        },
    );
}
