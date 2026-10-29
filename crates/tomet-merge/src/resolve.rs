//! Strips `@conflict` markers back out of a merged document while preserving
//! all untouched formatting, comments, and whitespace byte-for-byte.

use tomet_edit::{
    AttrSlot, ChildItem, EditDoc, EditValue, EditValueKind, InlineItem, NodeId, NodeKind,
    export_doc, import_source, parse_edit_value_str, parse_inline_items,
};

use crate::markers::{
    block_conflict_sides_from_slot, extract_conflict_block_side, extract_conflict_inline_side,
    unquote_scalar_display, value_conflict_sides_from_value,
};

/// Which side of one `@conflict` a caller picked, or that they edited a
/// fresh replacement by hand, or kept both versions. `A`/`B` match
/// `@conflict`'s own structural, non-perspectival names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolution {
    A,
    B,
    Both,
    /// The user wrote something new that is neither side verbatim.
    Rewritten(String),
}

/// Resolves `@conflict` markers in `src` according to `resolutions` (in document order),
/// leaving all source text outside resolved `@conflict` spans byte-identical.
pub fn resolve(src: &str, resolutions: &[ConflictResolution]) -> String {
    export_doc(&resolve_doc(&import_source(src), resolutions))
}

/// Resolves all `@conflict` markers in `src` to `resolution`, leaving all source
/// text outside `@conflict` spans byte-identical.
pub fn resolve_all(src: &str, resolution: ConflictResolution) -> String {
    export_doc(&resolve_all_doc(&import_source(src), resolution))
}

/// Resolves a single `@conflict` marker at `target_index` in `src` to `resolution`,
/// leaving all other conflicts and surrounding source text byte-identical.
pub fn resolve_single(src: &str, target_index: usize, resolution: ConflictResolution) -> String {
    export_doc(&resolve_single_doc(
        &import_source(src),
        target_index,
        resolution,
    ))
}

/// Resolves `@conflict` markers in `doc` according to `resolutions` (in document order).
pub fn resolve_doc(doc: &EditDoc, resolutions: &[ConflictResolution]) -> EditDoc {
    let mut out = doc.clone();
    apply_resolutions(&mut out, &mut resolutions.iter().map(Some));
    out
}

/// Resolves every `@conflict` marker in `doc` to `resolution`.
pub fn resolve_all_doc(doc: &EditDoc, resolution: ConflictResolution) -> EditDoc {
    let mut out = doc.clone();
    apply_resolutions(&mut out, &mut std::iter::repeat(Some(&resolution)));
    out
}

/// Resolves a single `@conflict` marker at `target_index` in `doc` to `resolution`.
pub fn resolve_single_doc(
    doc: &EditDoc,
    target_index: usize,
    resolution: ConflictResolution,
) -> EditDoc {
    let mut out = doc.clone();
    let mut idx = 0;
    apply_resolutions(
        &mut out,
        &mut std::iter::from_fn(|| {
            let res = (idx == target_index).then_some(&resolution);
            idx += 1;
            Some(res)
        }),
    );
    out
}

fn apply_resolutions<'a>(
    doc: &mut EditDoc,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) {
    let root_items = doc.root_children().to_vec();
    let (new_root, _) = resolve_child_seq(doc, &root_items, resolutions);
    *doc.root_children_mut() = new_root;
}

fn resolve_child_seq<'a>(
    doc: &mut EditDoc,
    items: &[ChildItem],
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> (Vec<ChildItem>, bool) {
    let mut out = Vec::with_capacity(items.len());
    let mut any_changed = false;
    let mut skip_one_leading_nl = false;

    for item in items {
        let Some(id) = item.as_node_id() else {
            if skip_one_leading_nl && let ChildItem::Trivia(t) = item {
                skip_one_leading_nl = false;
                let rest = t
                    .strip_prefix("\r\n")
                    .or_else(|| t.strip_prefix('\n'))
                    .unwrap_or(t);
                if !rest.is_empty() {
                    out.push(ChildItem::Trivia(rest.to_string()));
                }
                continue;
            }
            skip_one_leading_nl = false;
            out.push(item.clone());
            continue;
        };
        skip_one_leading_nl = false;
        let Some(node) = doc.get(id).cloned() else {
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
            match resolutions.next() {
                Some(Some(res)) => {
                    any_changed = true;
                    let a_text = extract_conflict_block_side(&a_inner);
                    let b_text = extract_conflict_block_side(&b_inner);
                    let replacement_src = match res {
                        ConflictResolution::A => a_text,
                        ConflictResolution::B => b_text,
                        ConflictResolution::Both => {
                            if a_text.is_empty() {
                                b_text
                            } else if b_text.is_empty() {
                                a_text
                            } else {
                                format!("{a_text}\n{b_text}")
                            }
                        }
                        ConflictResolution::Rewritten(text) => {
                            if text.ends_with('\n') {
                                text.clone()
                            } else {
                                format!("{text}\n")
                            }
                        }
                    };
                    let parsed = import_source(&replacement_src);
                    let cloned_items = doc.clone_children_from(&parsed, parsed.root_children());
                    out.extend(cloned_items);
                    skip_one_leading_nl = !replacement_src.is_empty();
                }
                Some(None) | None => out.push(ChildItem::Node(id)),
            }
            continue;
        }

        if resolve_node_in_place(doc, id, resolutions) {
            any_changed = true;
        }
        out.push(ChildItem::Node(id));
    }

    (out, any_changed)
}

fn resolve_node_in_place<'a>(
    doc: &mut EditDoc,
    id: NodeId,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> bool {
    let Some(node) = doc.get(id).cloned() else {
        return false;
    };

    match node.kind {
        NodeKind::Paragraph { .. } => {
            if let Some((new_text, new_inlines)) = resolve_inlines(&node.inlines, resolutions) {
                let n = doc.get_mut(id).unwrap();
                n.kind = NodeKind::Paragraph {
                    text: new_text.clone(),
                };
                n.raw = new_text;
                n.inlines = new_inlines;
                n.edited = true;
                return true;
            }
            false
        }
        NodeKind::Section {
            level,
            heading_raw,
            id_attr,
            children,
        } => {
            let title_resolved = resolve_inlines(&node.inlines, resolutions);
            let (new_children, children_changed) = resolve_child_seq(doc, &children, resolutions);
            if title_resolved.is_none() && !children_changed {
                return false;
            }
            let n = doc.get_mut(id).unwrap();
            let (final_heading_raw, final_inlines) = if let Some((new_t, new_inl)) = title_resolved
            {
                (
                    format!("{}{new_t}{}", n.heading_prefix, n.heading_suffix),
                    new_inl,
                )
            } else {
                (heading_raw, n.inlines.clone())
            };
            n.kind = NodeKind::Section {
                level,
                heading_raw: final_heading_raw,
                id_attr,
                children: new_children,
            };
            n.inlines = final_inlines;
            n.edited = true;
            true
        }
        NodeKind::BlockElement {
            name,
            declaration,
            id_attr,
            ..
        } => {
            let mut args_slot = node.slots.args_slot.clone();
            let mut data_slot = node.slots.data_slot.clone();
            let args_changed = args_slot
                .as_mut()
                .is_some_and(|s| resolve_attr_slot(s, resolutions));
            let mut content_changed = false;
            let new_content_children = node.slots.content_children.as_ref().map(|cc| {
                let (new_cc, ch) = resolve_child_seq(doc, cc, resolutions);
                content_changed = ch;
                new_cc
            });
            let data_changed = data_slot
                .as_mut()
                .is_some_and(|s| resolve_attr_slot(s, resolutions));

            if !args_changed && !content_changed && !data_changed {
                return false;
            }

            let n = doc.get_mut(id).unwrap();
            n.slots.args_slot = args_slot;
            n.slots.data_slot = data_slot;
            n.slots.content_children = new_content_children;
            if content_changed {
                n.slots.content_edited = true;
            }
            n.kind = NodeKind::BlockElement {
                name,
                declaration,
                id_attr,
            };
            n.edited = true;
            true
        }
        NodeKind::List { children } => {
            let (new_children, changed) = resolve_child_seq(doc, &children, resolutions);
            if changed {
                let n = doc.get_mut(id).unwrap();
                n.kind = NodeKind::List {
                    children: new_children,
                };
                n.edited = true;
            }
            changed
        }
        NodeKind::ListItem {
            marker,
            text,
            id_attr,
            children,
        } => {
            if children.iter().any(|c| c.as_node_id().is_some()) {
                let (new_children, changed) = resolve_child_seq(doc, &children, resolutions);
                if changed {
                    let n = doc.get_mut(id).unwrap();
                    n.kind = NodeKind::ListItem {
                        marker,
                        text,
                        id_attr,
                        children: new_children,
                    };
                    n.edited = true;
                }
                changed
            } else {
                let inlines = parse_inline_items(&text);
                if let Some((new_text, _)) = resolve_inlines(&inlines, resolutions) {
                    let n = doc.get_mut(id).unwrap();
                    n.kind = NodeKind::ListItem {
                        marker,
                        text: new_text.clone(),
                        id_attr,
                        children: vec![ChildItem::Trivia(new_text.clone())],
                    };
                    n.raw = new_text;
                    n.edited = true;
                    return true;
                }
                false
            }
        }
        _ => false,
    }
}

fn resolve_inlines<'a>(
    inlines: &[InlineItem],
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> Option<(String, Vec<InlineItem>)> {
    let mut out = String::new();
    let mut any_changed = false;

    for item in inlines {
        if let InlineItem::Element { name, slots, .. } = item
            && name == "conflict"
            && let Some(slot) = &slots.args_slot
            && let Some((a_inner, b_inner)) = block_conflict_sides_from_slot(slot)
        {
            match resolutions.next() {
                Some(Some(res)) => {
                    any_changed = true;
                    let a_str = extract_conflict_inline_side(&a_inner);
                    let b_str = extract_conflict_inline_side(&b_inner);
                    match res {
                        ConflictResolution::A => out.push_str(&a_str),
                        ConflictResolution::B => out.push_str(&b_str),
                        ConflictResolution::Both => {
                            if a_str.is_empty() {
                                out.push_str(&b_str);
                            } else if b_str.is_empty() {
                                out.push_str(&a_str);
                            } else if a_str.ends_with(char::is_whitespace)
                                || b_str.starts_with(char::is_whitespace)
                            {
                                out.push_str(&a_str);
                                out.push_str(&b_str);
                            } else {
                                out.push_str(&a_str);
                                out.push(' ');
                                out.push_str(&b_str);
                            }
                        }
                        ConflictResolution::Rewritten(text) => out.push_str(text),
                    }
                }
                Some(None) | None => out.push_str(item.raw_str()),
            }
            continue;
        }
        out.push_str(item.raw_str());
    }

    any_changed.then(|| {
        let parsed = parse_inline_items(&out);
        (out, parsed)
    })
}

fn resolve_attr_slot<'a>(
    slot: &mut AttrSlot,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> bool {
    let mut any_changed = false;
    for entry in &mut slot.entries {
        if resolve_edit_value(&mut entry.value, resolutions) {
            entry.edited = true;
            any_changed = true;
        }
    }
    if any_changed {
        slot.edited = true;
    }
    any_changed
}

fn resolve_edit_value<'a>(
    val: &mut EditValue,
    resolutions: &mut impl Iterator<Item = Option<&'a ConflictResolution>>,
) -> bool {
    if let Some((a_val, b_val)) = value_conflict_sides_from_value(val) {
        let a_cloned = a_val.clone();
        let b_cloned = b_val.clone();
        return match resolutions.next() {
            Some(Some(res)) => {
                *val = match res {
                    ConflictResolution::A => a_cloned,
                    ConflictResolution::B => b_cloned,
                    ConflictResolution::Both => {
                        let a_s = unquote_scalar_display(a_cloned.raw.trim());
                        let b_s = unquote_scalar_display(b_cloned.raw.trim());
                        if a_cloned.raw.trim().starts_with('"')
                            && b_cloned.raw.trim().starts_with('"')
                        {
                            parse_edit_value_str(&format!("\"{a_s} / {b_s}\""))
                        } else {
                            a_cloned
                        }
                    }
                    ConflictResolution::Rewritten(text) => {
                        parse_edit_value_str(&format!("\"{}\"", text.replace('"', "\\\"")))
                    }
                };
                val.edited = true;
                true
            }
            Some(None) | None => false,
        };
    }

    match &mut val.kind {
        EditValueKind::Map(slot) | EditValueKind::Seq(slot) => {
            if resolve_attr_slot(slot, resolutions) {
                val.edited = true;
                true
            } else {
                false
            }
        }
        _ => false,
    }
}
