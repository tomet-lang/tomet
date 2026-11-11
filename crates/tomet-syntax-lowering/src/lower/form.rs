//! Elements: `@name`, a block form, and the boxes `<?matcher...>` and `<$type...>`.
//!
//! Reading one form's head and its groups into an element, a hole or a type seat; the primary
//! group a nexus stands on is checked here, before any group is read.

use super::groups::lower_groups;
use super::{is_declaration, name_of, range_of, unsupported};
use crate::error::Result;
use crate::value::err;
use tomet_ast::{Block, Element, Hole, Inline, Name, Placement, TypeSeat, Value};
use tomet_cst::{AstNode, CstName, CstSlot, CstSlotMark, SyntaxKind as K, SyntaxNode};
use tomet_span::Span;
use tomet_tree::ElementExt;

/// A block form: `@name` and the groups that follow it.
pub(super) fn lower_block_form(node: &SyntaxNode) -> Result<Block> {
    if let Some(slot) = slot_of(node) {
        return lower_slot(&slot).map(LoweredSlot::into_block);
    }
    if is_declaration(node) {
        return Err(err(
            range_of(node).0,
            "a declaration belongs at the top of the file",
        ));
    }
    let element = lower_element(node)?.with_placement(Placement::Block);
    Ok(Block::Element(element))
}

pub(crate) fn lower_element(node: &SyntaxNode) -> Result<Element> {
    let sigil = node
        .children()
        .find(|n| n.kind() == K::SIGIL)
        .ok_or_else(|| unsupported("an element without a sigil"))?;
    let (sigil_start, _) = range_of(&sigil);
    if sigil.children_with_tokens().next().map(|e| e.kind()) != Some(K::AT) {
        return Err(unsupported("a sigil that is not `@`"));
    }
    let name = name_of(&sigil)?;
    // Before any group is read: a nameless `::(...)` would otherwise merge into the args.
    require_primary_group(node, &sigil.text().to_string(), sigil_start)?;
    let read = lower_groups(node)?;
    // The element ends with its last group; the white space and comments the
    // node also holds are not part of it.
    let end = node
        .children()
        .map(|n| range_of(&n).1)
        .max()
        .unwrap_or(sigil_start);
    Ok(Element {
        name,
        placement: Placement::Inline,
        data: read.data,
        content: read.content,
        id: read.id,
        connects: read.connects,
        span: Span::new(sigil_start, end),
    })
}

/// A nexus -- `@name`, a `::name` declaration, a `::name` connect -- stands on a primary group: `(args)` or `[content]`
/// (a `|` run is the latter). `{value}`, `#(id)` and a `::connect` only add to one, so none of
/// them can be all a nexus has. `head` is how the nexus is written, `at` where it starts.
///
/// Only the groups written on `node` itself count: those of a nameless `::` connect merge into the
/// owner's after the fact, and a nexus does not stand on a group it borrows.
pub(super) fn require_primary_group(node: &SyntaxNode, head: &str, at: usize) -> Result<()> {
    if node
        .children()
        .any(|n| matches!(n.kind(), K::ARGS | K::CONTENT))
    {
        return Ok(());
    }
    let mut lone: Vec<&str> = Vec::new();
    for child in node.children() {
        let group = match child.kind() {
            K::VALUE_DATA | K::MAP_LITERAL => "`{...}`",
            K::ID_GROUP => "`#(id)`",
            K::CONNECT => "`::connect`",
            _ => continue,
        };
        if !lone.contains(&group) {
            lone.push(group);
        }
    }
    let message = if lone.is_empty() {
        format!(
            "`{head}` needs a primary group, `(...)` or `[...]`: write `{head}()` for an empty one"
        )
    } else {
        format!(
            "`{head}` needs a primary group, `(...)` or `[...]`: {} cannot stand without one",
            lone.join(", ")
        )
    };
    Err(err(at, message))
}

/// The box an element node holds, if it is one: `<?matcher...>` and `<$type...>` are elements of
/// the tree, where a name tag (`<name>`) is a `SLOT` among the text and never wrapped.
pub(crate) fn slot_of(node: &SyntaxNode) -> Option<SyntaxNode> {
    node.children().find(|n| n.kind() == K::SLOT)
}

/// What a marked box lowers to. The AST has a node for each, and no node for the family: this is
/// how the places that take either (a block, running text, a value) read one box and place it.
pub(crate) enum LoweredSlot {
    Hole(Hole),
    TypeSeat(TypeSeat),
}

impl LoweredSlot {
    /// The box standing as a block.
    pub(crate) fn into_block(self) -> Block {
        match self {
            LoweredSlot::Hole(mut hole) => {
                hole.placement = Placement::Block;
                Block::Hole(hole)
            }
            LoweredSlot::TypeSeat(seat) => Block::TypeSeat(seat),
        }
    }

    /// The box standing in running text.
    pub(crate) fn into_inline(self) -> Inline {
        match self {
            LoweredSlot::Hole(hole) => Inline::Hole(hole),
            LoweredSlot::TypeSeat(seat) => Inline::TypeSeat(seat),
        }
    }

    /// The box standing where a value goes.
    pub(crate) fn into_value(self) -> Value {
        match self {
            LoweredSlot::Hole(hole) => Value::Hole(Box::new(hole)),
            LoweredSlot::TypeSeat(seat) => Value::TypeSeat(seat),
        }
    }
}

/// A marked box, from its `SLOT` node: `<?matcher(args){value}>[content]` is a hole and
/// `<$type(args)>` a type seat. The mark says which.
pub(crate) fn lower_slot(slot: &SyntaxNode) -> Result<LoweredSlot> {
    let cst = CstSlot::cast(slot.clone()).expect("a SLOT node");
    match cst.mark() {
        CstSlotMark::Hole => lower_hole(slot).map(LoweredSlot::Hole),
        CstSlotMark::TypeSeat => crate::interp::lower_type_seat(slot).map(LoweredSlot::TypeSeat),
        CstSlotMark::NameTag => Err(unsupported("a name tag as a form")),
    }
}

/// `<?matcher(args){value}>[content]`: a hole, from the `SLOT` node of a box marked with `?`.
fn lower_hole(slot: &SyntaxNode) -> Result<Hole> {
    let (start, end) = range_of(slot);
    let matcher = slot
        .children()
        .find(|n| n.kind() == K::NAME)
        .and_then(CstName::cast)
        .map(|c| {
            let (namespace, name) = c.parts();
            Name { namespace, name }
        });
    let read = lower_groups(slot)?;
    Ok(Hole {
        matcher,
        placement: Placement::Inline,
        data: read.data,
        content: read.content,
        span: Span::new(start, end),
    })
}
