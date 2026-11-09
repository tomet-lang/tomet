//! The groups of a form: `(args)`, `{value}`, `[content]`, `#(id)` and the `::connect`s.
//!
//! Whatever order they were written in. What a group holds is [`crate::element`]'s.

use super::connect::{
    desugar_connect, is_merge_connect, lower_connect, merge_connect, refuse_single_colon,
};
use super::error_node::lower_error;
use super::form::{lower_element, lower_slot, slot_of};
use super::{lower_blocks, range_of, unsupported};
use crate::element::Groups;
use crate::error::Result;
use tomet_ast::{Block, Value};
use tomet_cst::{SyntaxKind as K, SyntaxNode};

/// The groups of `node`: `(args)`, `{value}`, `[content]`, `#(id)` and the
/// `::connect`s, whatever order they were written in.
pub(super) fn lower_groups(node: &SyntaxNode) -> Result<Groups> {
    lower_group_list(node.children())
}

/// The groups in `nodes`, read as [`lower_groups`] reads an element's.
pub(crate) fn lower_group_list(nodes: impl IntoIterator<Item = SyntaxNode>) -> Result<Groups> {
    let mut read = Groups::default();
    for child in nodes {
        match child.kind() {
            K::SIGIL | K::SECTION_HEADING | K::NAME | K::BODY => {}
            // What a list item holds besides groups: its sublist and the inline
            // nodes of its text, lowered by the item itself.
            K::LIST
            | K::INLINE_FORM
            | K::SLOT
            | K::ESCAPE
            | K::EMPHASIS
            | K::STRONG
            | K::STRIKEOUT
            | K::CODE_SPAN
            | K::AUTOLINK => {}
            K::ARGS => {
                read.data.args = Some(crate::element::lower_paren_value(&child)?);
            }
            K::VALUE_DATA | K::MAP_LITERAL => {
                read.data.value = Some(crate::element::lower_value_group(&child)?);
            }
            K::CONTENT => read.content = Some(lower_content(&child)?),
            K::ID_GROUP => {
                read.id = Some(crate::element::lower_hash_id(&child)?);
            }
            K::CONNECT => {
                refuse_single_colon(&child)?;
                if is_merge_connect(&child) {
                    merge_connect(&child, &mut read)?;
                } else if !desugar_connect(&child, &mut read)? {
                    read.connects.push(lower_connect(&child)?);
                }
            }
            K::ERROR => return Err(lower_error(&child)),
            other => return Err(unsupported(format!("{other:?} in an element's groups"))),
        }
    }
    Ok(read)
}

/// What a value embeds at `node`, and where it ends: an element or a hole
/// (`@name...`, `?matcher(...)`), or `[content]` / a `|` run as blocks.
pub(crate) fn lower_embedded(node: &SyntaxNode) -> Result<(Value, usize)> {
    match node.kind() {
        K::CONTENT => {
            let (_, end) = range_of(node);
            Ok((Value::Blocks(lower_content(node)?), end))
        }
        _ if slot_of(node).is_some() => {
            let slot = slot_of(node).expect("checked");
            let hole = lower_slot(&slot)?;
            let end = hole.span.end;
            Ok((Value::Hole(Box::new(hole)), end))
        }
        _ => {
            let element = lower_element(node)?;
            let end = element.span.end;
            Ok((Value::Element(Box::new(element)), end))
        }
    }
}

/// `[content]`: the blocks between the brackets, or the blocks of a `|` run.
pub(crate) fn lower_content(node: &SyntaxNode) -> Result<Vec<Block>> {
    if let Some(err_node) = node
        .children()
        .find(|c| c.kind() == K::ERROR && c.children_with_tokens().count() == 0)
    {
        return Err(lower_error(&err_node));
    }
    lower_blocks(node)
}
