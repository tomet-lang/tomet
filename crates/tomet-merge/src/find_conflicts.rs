//! Reads `@conflict` markers back out of a document without removing
//! them -- for showing the user what's in conflict (the resolution UI)
//! and for post-merge conflict reporting.

use tomet_edit::{
    AttrSlot, ChildItem, EditDoc, EditValue, EditValueKind, InlineItem, Node, NodeKind,
    import_source, parse_inline_items,
};

use crate::markers::{
    block_conflict_sides_from_slot, extract_conflict_block_side, extract_conflict_inline_side,
    unquote_scalar_display, value_conflict_sides_from_value,
};

/// An individual conflict region discovered within a document. `a`/`b`
/// are the same structural, non-perspectival names `@conflict` itself
/// uses -- neither is "mine": a conflict found cold (the app reopened,
/// or a different device than the one that ran the merge) has no
/// stored way to tell which side was whose. A caller that wants a
/// mine/theirs label derives it itself, live, from something `@conflict`
/// doesn't carry (such as the caller's resolution UI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictItem {
    pub index: usize,
    pub a: String,
    pub b: String,
}

/// Finds and extracts all `@conflict` regions present in `src`.
pub fn find_conflicts(src: &str) -> Vec<ConflictItem> {
    find_conflicts_doc(&import_source(src))
}

/// Returns `true` if `src` contains any unresolved `@conflict` markers.
pub fn has_conflicts(src: &str) -> bool {
    has_conflicts_doc(&import_source(src))
}

/// Finds and extracts all `@conflict` regions present in `doc`.
pub fn find_conflicts_doc(doc: &EditDoc) -> Vec<ConflictItem> {
    let mut items = Vec::new();
    find_conflicts_in_children(doc, doc.root_children(), &mut items);
    items
}

/// Returns `true` if `doc` contains any unresolved `@conflict` markers.
pub fn has_conflicts_doc(doc: &EditDoc) -> bool {
    !find_conflicts_doc(doc).is_empty()
}

fn find_conflicts_in_children(doc: &EditDoc, items: &[ChildItem], out: &mut Vec<ConflictItem>) {
    for item in items {
        let Some(id) = item.as_node_id() else {
            continue;
        };
        let Some(node) = doc.get(id) else {
            continue;
        };
        if let NodeKind::BlockElement {
            name,
            declaration: false,
            ..
        } = &node.kind
            && name == "conflict"
            && let Some(args_slot) = &node.slots.args_slot
            && let Some((a_inner, b_inner)) = block_conflict_sides_from_slot(args_slot)
        {
            out.push(ConflictItem {
                index: out.len(),
                a: extract_conflict_block_side(&a_inner).trim().to_string(),
                b: extract_conflict_block_side(&b_inner).trim().to_string(),
            });
            continue;
        }
        find_conflicts_in_node(doc, node, out);
    }
}

fn find_conflicts_in_node(doc: &EditDoc, node: &Node, out: &mut Vec<ConflictItem>) {
    match &node.kind {
        NodeKind::Paragraph { .. } => {
            find_conflicts_in_inlines(&node.inlines, out);
        }
        NodeKind::Section { children, .. } => {
            find_conflicts_in_inlines(&node.inlines, out);
            find_conflicts_in_children(doc, children, out);
        }
        NodeKind::BlockElement { .. } => {
            if let Some(args_slot) = &node.slots.args_slot {
                find_conflicts_in_attr_slot(args_slot, out);
            }
            if let Some(content_children) = &node.slots.content_children {
                find_conflicts_in_children(doc, content_children, out);
            }
            if let Some(data_slot) = &node.slots.data_slot {
                find_conflicts_in_attr_slot(data_slot, out);
            }
        }
        NodeKind::List { children } => {
            find_conflicts_in_children(doc, children, out);
        }
        NodeKind::ListItem { text, children, .. } => {
            if children.iter().any(|c| c.as_node_id().is_some()) {
                find_conflicts_in_children(doc, children, out);
            } else {
                find_conflicts_in_inlines(&parse_inline_items(text), out);
            }
        }
        _ => {}
    }
}

fn find_conflicts_in_inlines(inlines: &[InlineItem], out: &mut Vec<ConflictItem>) {
    for item in inlines {
        if let InlineItem::Element { name, slots, .. } = item
            && name == "conflict"
            && let Some(slot) = &slots.args_slot
            && let Some((a_inner, b_inner)) = block_conflict_sides_from_slot(slot)
        {
            out.push(ConflictItem {
                index: out.len(),
                a: extract_conflict_inline_side(&a_inner),
                b: extract_conflict_inline_side(&b_inner),
            });
        }
    }
}

fn find_conflicts_in_attr_slot(slot: &AttrSlot, out: &mut Vec<ConflictItem>) {
    for entry in &slot.entries {
        find_conflicts_in_edit_value(&entry.value, out);
    }
}

fn find_conflicts_in_edit_value(val: &EditValue, out: &mut Vec<ConflictItem>) {
    if let Some((a, b)) = value_conflict_sides_from_value(val) {
        out.push(ConflictItem {
            index: out.len(),
            a: unquote_scalar_display(a.raw.trim()),
            b: unquote_scalar_display(b.raw.trim()),
        });
        return;
    }
    match &val.kind {
        EditValueKind::Map(slot) | EditValueKind::Seq(slot) => {
            find_conflicts_in_attr_slot(slot, out);
        }
        _ => {}
    }
}
