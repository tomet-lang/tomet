//! Format-preserving three-way merge over [`tomet_edit::EditDoc`] and `.tmt` source text.

use tomet_edit::{
    ChildItem, EditDoc, Node, NodeId, NodeKind, export_children_str, export_doc, export_node_str,
    import_source, parse_inline_items,
};

use crate::attrs::merge_optional_attr_slot;
use crate::conflict::has_conflicts;
use crate::diff3;
use crate::markers::{BlockUnit, UnitKind, build_conflict_block_unit};
use crate::prose::merge_inline_items;
use crate::trivia::{decompose_children, merge_ws, propagate_matched_leading_ws};

/// The outcome of a format-preserving three-way merge.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The merged `.tmt` source text, preserving untouched formatting, comments,
    /// inline sugar, and whitespace byte-for-byte.
    pub text: String,
    /// The merged [`EditDoc`] tree.
    pub doc: EditDoc,
    /// `true` if merging succeeded without leaving any `@conflict` markers.
    pub clean: bool,
}

/// Performs a format-preserving three-way merge of `base`, `local`, and `remote`
/// `.tmt` source texts.
pub fn merge(base: &str, local: &str, remote: &str) -> MergeResult {
    if local == remote || base == remote {
        let doc = import_source(local);
        let clean = !has_conflicts(local);
        return MergeResult {
            text: local.to_string(),
            doc,
            clean,
        };
    }
    if base == local {
        let doc = import_source(remote);
        let clean = !has_conflicts(remote);
        return MergeResult {
            text: remote.to_string(),
            doc,
            clean,
        };
    }
    merge_doc(
        &import_source(base),
        &import_source(local),
        &import_source(remote),
    )
}

/// Performs a format-preserving three-way merge of three [`EditDoc`] instances.
pub fn merge_doc(base: &EditDoc, local: &EditDoc, remote: &EditDoc) -> MergeResult {
    let base_src = export_doc(base);
    let local_src = export_doc(local);
    let remote_src = export_doc(remote);

    if local_src == remote_src || base_src == remote_src {
        let clean = !has_conflicts(&local_src);
        return MergeResult {
            text: local_src,
            doc: local.clone(),
            clean,
        };
    }
    if base_src == local_src {
        let clean = !has_conflicts(&remote_src);
        return MergeResult {
            text: remote_src,
            doc: remote.clone(),
            clean,
        };
    }

    let mut merger = Merger {
        base,
        local,
        remote,
        out: EditDoc::new(),
    };
    let (root_children, clean) = merger
        .merge_child_seq(
            base.root_children(),
            local.root_children(),
            remote.root_children(),
            true,
            true,
        )
        .expect("top-level merge_child_seq with allow_conflict_blocks=true never returns None");

    *merger.out.root_children_mut() = root_children;
    let text = export_doc(&merger.out);
    let doc = import_source(&text);
    MergeResult { text, doc, clean }
}

#[derive(Clone)]
struct Merger<'a> {
    base: &'a EditDoc,
    local: &'a EditDoc,
    remote: &'a EditDoc,
    out: EditDoc,
}

impl<'a> Merger<'a> {
    fn clone_unit(&mut self, unit: &BlockUnit) -> BlockUnit {
        match &unit.kind {
            UnitKind::Comment(c) => BlockUnit {
                leading_ws: unit.leading_ws.clone(),
                kind: UnitKind::Comment(c.clone()),
            },
            UnitKind::Node {
                id,
                raw,
                trailing_nl,
            } => {
                let src = if self.local.get(*id).is_some_and(|n| &n.raw == raw) {
                    self.local
                } else if self.remote.get(*id).is_some_and(|n| &n.raw == raw) {
                    self.remote
                } else if self.base.get(*id).is_some_and(|n| &n.raw == raw) {
                    self.base
                } else if self.local.get(*id).is_some() {
                    self.local
                } else if self.remote.get(*id).is_some() {
                    self.remote
                } else {
                    self.base
                };
                let new_id = self.out.clone_node_from(src, *id);
                BlockUnit {
                    leading_ws: unit.leading_ws.clone(),
                    kind: UnitKind::Node {
                        id: new_id,
                        raw: raw.clone(),
                        trailing_nl: trailing_nl.clone(),
                    },
                }
            }
        }
    }

    fn merge_child_seq(
        &mut self,
        b_items: &[ChildItem],
        l_items: &[ChildItem],
        r_items: &[ChildItem],
        allow_conflict_blocks: bool,
        is_root_or_content: bool,
    ) -> Option<(Vec<ChildItem>, bool)> {
        let (mut b_units, b_trail) = decompose_children(self.base, b_items);
        let (mut l_units, l_trail) = decompose_children(self.local, l_items);
        let (mut r_units, r_trail) = decompose_children(self.remote, r_items);

        let b_starts_empty_ws = b_units.first().is_none_or(|u| u.leading_ws.is_empty());
        let l_starts_empty_ws = l_units.first().is_none_or(|u| u.leading_ws.is_empty());
        let r_starts_empty_ws = r_units.first().is_none_or(|u| u.leading_ws.is_empty());
        let default_sep = l_units
            .iter()
            .skip(1)
            .chain(r_units.iter().skip(1))
            .chain(b_units.iter().skip(1))
            .find_map(|u| (!u.leading_ws.is_empty()).then(|| u.leading_ws.clone()))
            .unwrap_or_else(|| "\n".to_string());

        propagate_matched_leading_ws(&mut b_units, &mut l_units, &mut r_units);

        let mut merged_units: Vec<BlockUnit> = Vec::new();
        let mut clean = true;

        for region in diff3::merge3(&b_units, &l_units, &r_units) {
            match region {
                diff3::Region::Same(units) => {
                    for u in &units {
                        merged_units.push(self.clone_unit(u));
                    }
                }
                diff3::Region::Conflict { base, mine, theirs } => {
                    if !base.is_empty() && base.len() == mine.len() && mine.len() == theirs.len() {
                        let mut sub = self.clone();
                        let mut narrowed_all = Vec::with_capacity(base.len());
                        let mut all_ok = true;
                        let mut sub_clean_all = true;

                        for ((bu, lu), ru) in base.iter().zip(mine.iter()).zip(theirs.iter()) {
                            if let Some((nu, sc)) = sub.merge_one_unit(bu, lu, ru) {
                                narrowed_all.push(nu);
                                sub_clean_all &= sc;
                            } else if allow_conflict_blocks {
                                let leading_ws =
                                    merge_ws(&bu.leading_ws, &lu.leading_ws, &ru.leading_ws);
                                narrowed_all.push(build_conflict_block_unit(
                                    std::slice::from_ref(lu),
                                    std::slice::from_ref(ru),
                                    leading_ws,
                                    &mut sub.out,
                                ));
                                sub_clean_all = false;
                            } else {
                                all_ok = false;
                                break;
                            }
                        }

                        if all_ok {
                            self.out = sub.out;
                            merged_units.extend(narrowed_all);
                            clean &= sub_clean_all;
                            continue;
                        }
                    }

                    if !allow_conflict_blocks {
                        return None;
                    }

                    clean = false;
                    let leading_ws = match (base.first(), mine.first(), theirs.first()) {
                        (Some(b), Some(l), Some(r)) => {
                            merge_ws(&b.leading_ws, &l.leading_ws, &r.leading_ws)
                        }
                        (_, Some(l), _) => l.leading_ws.clone(),
                        (_, _, Some(r)) => r.leading_ws.clone(),
                        _ => String::new(),
                    };
                    merged_units.push(build_conflict_block_unit(
                        &mine,
                        &theirs,
                        leading_ws,
                        &mut self.out,
                    ));
                }
            }
        }

        if is_root_or_content
            && b_starts_empty_ws
            && l_starts_empty_ws
            && r_starts_empty_ws
            && let Some(first) = merged_units.first_mut()
            && first.leading_ws == "\n"
        {
            first.leading_ws.clear();
        }
        for idx in 1..merged_units.len() {
            let prev_ends_with_nl = match &merged_units[idx - 1].kind {
                UnitKind::Comment(c) => c.ends_with('\n'),
                UnitKind::Node {
                    raw, trailing_nl, ..
                } => raw.ends_with('\n') || trailing_nl.ends_with('\n'),
            };
            if merged_units[idx].leading_ws.is_empty() && prev_ends_with_nl {
                let was_at_zero = l_units
                    .first()
                    .is_some_and(|u| u == &merged_units[idx] && u.leading_ws.is_empty())
                    || r_units
                        .first()
                        .is_some_and(|u| u == &merged_units[idx] && u.leading_ws.is_empty());
                if was_at_zero {
                    merged_units[idx].leading_ws = default_sep.clone();
                }
            }
        }

        let merged_trail = merge_ws(&b_trail, &l_trail, &r_trail);
        let mut out_items = Vec::new();
        for u in merged_units {
            if !u.leading_ws.is_empty() {
                out_items.push(ChildItem::Trivia(u.leading_ws));
            }
            match u.kind {
                UnitKind::Comment(c) => out_items.push(ChildItem::Trivia(c)),
                UnitKind::Node {
                    id, trailing_nl, ..
                } => {
                    out_items.push(ChildItem::Node(id));
                    if !trailing_nl.is_empty() {
                        out_items.push(ChildItem::Trivia(trailing_nl));
                    }
                }
            }
        }
        if !merged_trail.is_empty() {
            out_items.push(ChildItem::Trivia(merged_trail));
        }

        Some((out_items, clean))
    }

    fn merge_one_unit(
        &mut self,
        b_unit: &BlockUnit,
        l_unit: &BlockUnit,
        r_unit: &BlockUnit,
    ) -> Option<(BlockUnit, bool)> {
        let leading_ws = merge_ws(&b_unit.leading_ws, &l_unit.leading_ws, &r_unit.leading_ws);
        let (
            UnitKind::Node {
                id: bid,
                raw: b_raw,
                trailing_nl: b_nl,
            },
            UnitKind::Node {
                id: lid,
                raw: l_raw,
                trailing_nl: l_nl,
            },
            UnitKind::Node {
                id: rid,
                raw: r_raw,
                trailing_nl: r_nl,
            },
        ) = (&b_unit.kind, &l_unit.kind, &r_unit.kind)
        else {
            return None;
        };

        if l_raw == r_raw || b_raw == r_raw {
            let new_id = self.out.clone_node_from(self.local, *lid);
            return Some((
                BlockUnit {
                    leading_ws,
                    kind: UnitKind::Node {
                        id: new_id,
                        raw: l_raw.clone(),
                        trailing_nl: l_nl.clone(),
                    },
                },
                true,
            ));
        }
        if b_raw == l_raw {
            let new_id = self.out.clone_node_from(self.remote, *rid);
            return Some((
                BlockUnit {
                    leading_ws,
                    kind: UnitKind::Node {
                        id: new_id,
                        raw: r_raw.clone(),
                        trailing_nl: r_nl.clone(),
                    },
                },
                true,
            ));
        }

        let (merged_id, clean) = self.merge_one_node(*bid, *lid, *rid)?;
        let merged_raw = export_node_str(&self.out, merged_id);
        let trailing_nl = if merged_raw.ends_with('\n') {
            String::new()
        } else {
            merge_ws(b_nl, l_nl, r_nl)
        };
        Some((
            BlockUnit {
                leading_ws,
                kind: UnitKind::Node {
                    id: merged_id,
                    raw: merged_raw,
                    trailing_nl,
                },
            },
            clean,
        ))
    }

    fn merge_one_node(&mut self, bid: NodeId, lid: NodeId, rid: NodeId) -> Option<(NodeId, bool)> {
        let b_node = self.base.get(bid)?;
        let l_node = self.local.get(lid)?;
        let r_node = self.remote.get(rid)?;

        match (&b_node.kind, &l_node.kind, &r_node.kind) {
            (
                NodeKind::Paragraph { .. },
                NodeKind::Paragraph { .. },
                NodeKind::Paragraph { .. },
            ) => {
                let (merged_text, merged_inlines, clean) =
                    merge_inline_items(&b_node.inlines, &l_node.inlines, &r_node.inlines);
                let new_id = self.out.alloc_id();
                let mut node = Node::new(
                    new_id,
                    NodeKind::Paragraph {
                        text: merged_text.clone(),
                    },
                    merged_text,
                );
                node.inlines = merged_inlines;
                node.edited = true;
                self.out.insert_node(node);
                Some((new_id, clean))
            }
            (
                NodeKind::Section {
                    level: bl,
                    heading_raw: bh,
                    id_attr: bid_attr,
                    children: bc,
                },
                NodeKind::Section {
                    level: ll,
                    heading_raw: lh,
                    id_attr: lid_attr,
                    children: lc,
                },
                NodeKind::Section {
                    level: rl,
                    heading_raw: rh,
                    id_attr: rid_attr,
                    children: rc,
                },
            ) if bl == ll && ll == rl && lid_attr == rid_attr && bid_attr == lid_attr => {
                let (heading_raw, inlines, heading_prefix, heading_suffix, title_clean) =
                    if lh == rh || bh == rh {
                        (
                            lh.clone(),
                            l_node.inlines.clone(),
                            l_node.heading_prefix.clone(),
                            l_node.heading_suffix.clone(),
                            true,
                        )
                    } else if bh == lh {
                        (
                            rh.clone(),
                            r_node.inlines.clone(),
                            r_node.heading_prefix.clone(),
                            r_node.heading_suffix.clone(),
                            true,
                        )
                    } else {
                        let (merged_title, merged_inlines, clean) =
                            merge_inline_items(&b_node.inlines, &l_node.inlines, &r_node.inlines);
                        (
                            format!(
                                "{}{merged_title}{}",
                                l_node.heading_prefix, l_node.heading_suffix
                            ),
                            merged_inlines,
                            l_node.heading_prefix.clone(),
                            l_node.heading_suffix.clone(),
                            clean,
                        )
                    };

                let (merged_children, children_clean) =
                    self.merge_child_seq(bc, lc, rc, true, false)?;
                let new_id = self.out.alloc_id();
                let mut node = Node::new(
                    new_id,
                    NodeKind::Section {
                        level: *ll,
                        heading_raw,
                        id_attr: lid_attr.clone(),
                        children: merged_children,
                    },
                    String::new(),
                );
                node.inlines = inlines;
                node.heading_prefix = heading_prefix;
                node.heading_suffix = heading_suffix;
                node.edited = true;
                self.out.insert_node(node);
                Some((new_id, title_clean && children_clean))
            }
            (
                NodeKind::BlockForm {
                    name: bn,
                    declaration: bd,
                    id_attr: bid_attr,
                    ..
                },
                NodeKind::BlockForm {
                    name: ln,
                    declaration: ld,
                    id_attr: lid_attr,
                    ..
                },
                NodeKind::BlockForm {
                    name: rn,
                    declaration: rd,
                    id_attr: rid_attr,
                    ..
                },
            ) if bn == ln && ln == rn && bd == ld && ld == rd && lid_attr == rid_attr => {
                self.merge_block_form(b_node, l_node, r_node)
            }
            (
                NodeKind::List { children: bc },
                NodeKind::List { children: lc },
                NodeKind::List { children: rc },
            ) => {
                let (merged_children, clean) = self.merge_child_seq(bc, lc, rc, false, false)?;
                let new_id = self.out.alloc_id();
                let mut node = Node::new(
                    new_id,
                    NodeKind::List {
                        children: merged_children,
                    },
                    String::new(),
                );
                node.edited = true;
                self.out.insert_node(node);
                Some((new_id, clean))
            }
            (
                NodeKind::ListItem {
                    marker: bm,
                    text: bt,
                    id_attr: bid_attr,
                    children: bc,
                },
                NodeKind::ListItem {
                    marker: lm,
                    text: lt,
                    id_attr: lid_attr,
                    children: lc,
                },
                NodeKind::ListItem {
                    marker: rm,
                    text: rt,
                    id_attr: rid_attr,
                    children: rc,
                },
            ) if bm == lm && lm == rm && lid_attr == rid_attr && bid_attr == lid_attr => {
                if bc
                    .iter()
                    .chain(lc)
                    .chain(rc)
                    .any(|c| c.as_node_id().is_some())
                {
                    return None;
                }
                let (b_prefix, b_body) = split_list_item_prefix(bt, bm)?;
                let (l_prefix, l_body) = split_list_item_prefix(lt, lm)?;
                let (r_prefix, r_body) = split_list_item_prefix(rt, rm)?;
                if b_prefix != l_prefix || l_prefix != r_prefix {
                    return None;
                }
                let (merged_body, _, clean) = merge_inline_items(
                    &parse_inline_items(b_body),
                    &parse_inline_items(l_body),
                    &parse_inline_items(r_body),
                );
                let merged_text = format!("{l_prefix}{merged_body}");
                let new_id = self.out.alloc_id();
                let mut node = Node::new(
                    new_id,
                    NodeKind::ListItem {
                        marker: lm.clone(),
                        text: merged_text.clone(),
                        id_attr: lid_attr.clone(),
                        children: vec![ChildItem::Trivia(merged_text.clone())],
                    },
                    merged_text,
                );
                node.edited = true;
                self.out.insert_node(node);
                Some((new_id, clean))
            }
            _ => None,
        }
    }

    fn merge_block_form(
        &mut self,
        b_node: &Node,
        l_node: &Node,
        r_node: &Node,
    ) -> Option<(NodeId, bool)> {
        let NodeKind::BlockForm {
            name,
            declaration,
            id_attr,
            ..
        } = &l_node.kind
        else {
            return None;
        };

        let (args_slot, args_clean) = merge_optional_attr_slot(
            &b_node.slots.args_slot,
            &l_node.slots.args_slot,
            &r_node.slots.args_slot,
        )?;

        let (content_children, content_edited, content_clean) = match (
            &b_node.slots.content_children,
            &l_node.slots.content_children,
            &r_node.slots.content_children,
        ) {
            (Some(bc), Some(lc), Some(rc)) => {
                let bc_raw = export_children_str(self.base, bc);
                let lc_raw = export_children_str(self.local, lc);
                let rc_raw = export_children_str(self.remote, rc);
                if lc_raw == rc_raw || bc_raw == rc_raw {
                    (
                        Some(self.out.clone_children_from(self.local, lc)),
                        false,
                        true,
                    )
                } else if bc_raw == lc_raw {
                    (
                        Some(self.out.clone_children_from(self.remote, rc)),
                        true,
                        true,
                    )
                } else {
                    let (merged, clean) = self.merge_child_seq(bc, lc, rc, true, true)?;
                    (Some(merged), true, clean)
                }
            }
            (None, None, None) => (None, false, true),
            _ => return None,
        };

        let (data_slot, data_clean) = merge_optional_attr_slot(
            &b_node.slots.data_slot,
            &l_node.slots.data_slot,
            &r_node.slots.data_slot,
        )?;

        let mut slots = l_node.slots.clone();
        slots.args_slot = args_slot;
        slots.data_slot = data_slot;
        slots.content_children = content_children;
        slots.content_edited = content_edited;
        if slots.args_slot.is_some() {
            slots.ensure_piece_args();
        }
        if slots.data_slot.is_some() {
            slots.ensure_piece_data();
        }

        let new_id = self.out.alloc_id();
        let mut new_node = Node::new(
            new_id,
            NodeKind::BlockForm {
                name: name.clone(),
                declaration: *declaration,
                id_attr: id_attr.clone(),
            },
            String::new(),
        );
        new_node.slots = slots;
        new_node.edited = true;
        self.out.insert_node(new_node);

        Some((new_id, args_clean && content_clean && data_clean))
    }
}

fn split_list_item_prefix<'a>(text: &'a str, marker: &str) -> Option<(&'a str, &'a str)> {
    let ws_len = text.len() - text.trim_start_matches([' ', '\t']).len();
    let after_marker = text[ws_len..].strip_prefix(marker)?;
    let post_ws_len = after_marker.len() - after_marker.trim_start_matches([' ', '\t']).len();
    let total_prefix = ws_len + marker.len() + post_ws_len;
    Some((&text[..total_prefix], &text[total_prefix..]))
}
