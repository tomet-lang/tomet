//! How the attribute slots and values of a node are merged.

use std::collections::BTreeSet;

use crate::markers::value_side_for_marker;
use tomet_edit::{AttrEntry, AttrSlot, EditValue, EditValueKind, parse_edit_value_str};

pub(crate) fn merge_optional_attr_slot(
    base: &Option<AttrSlot>,
    local: &Option<AttrSlot>,
    remote: &Option<AttrSlot>,
) -> Option<(Option<AttrSlot>, bool)> {
    match (base, local, remote) {
        (Some(b), Some(l), Some(r)) => {
            let (merged, clean) = merge_attr_slot(b, l, r)?;
            Some((Some(merged), clean))
        }
        (None, None, None) => Some((None, true)),
        _ => None,
    }
}

fn merge_attr_slot(b: &AttrSlot, l: &AttrSlot, r: &AttrSlot) -> Option<(AttrSlot, bool)> {
    if l.raw == r.raw || b.raw == r.raw {
        return Some((l.clone(), true));
    }
    if b.raw == l.raw {
        return Some((r.clone(), true));
    }

    if l.open_delim == "{"
        && b.entries
            .iter()
            .chain(&l.entries)
            .chain(&r.entries)
            .any(|e| e.key.is_empty())
    {
        return None;
    }

    let style_donor = if !l.entries.is_empty() {
        l
    } else if !r.entries.is_empty() {
        r
    } else {
        b
    };
    let leading_trivia = style_donor.leading_trivia.clone();
    let sep_lead = style_donor.infer_new_entry_sep();
    let trailing_trivia = style_donor.trailing_trivia.clone();

    let mut order: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in l.entries.iter().chain(&r.entries) {
        if seen.insert(entry.key.clone()) {
            order.push(entry.key.clone());
        }
    }

    fn lookup<'a>(entries: &'a [AttrEntry], key: &str) -> Option<&'a AttrEntry> {
        entries.iter().find(|e| e.key == key)
    }
    let mut out_entries: Vec<AttrEntry> = Vec::new();
    let mut clean = true;

    for key in &order {
        let (be, le, re) = (
            lookup(&b.entries, key),
            lookup(&l.entries, key),
            lookup(&r.entries, key),
        );
        match (le, re) {
            (Some(l_ent), None) => {
                if be.is_none_or(|b_ent| b_ent.value.raw.trim() != l_ent.value.raw.trim()) {
                    out_entries.push(l_ent.clone());
                }
            }
            (None, Some(r_ent)) => {
                if be.is_none_or(|b_ent| b_ent.value.raw.trim() != r_ent.value.raw.trim()) {
                    out_entries.push(r_ent.clone());
                }
            }
            (None, None) => unreachable!(),
            (Some(l_ent), Some(r_ent)) => {
                let l_val = l_ent.value.raw.trim();
                let r_val = r_ent.value.raw.trim();
                let b_val = be.map(|e| e.value.raw.trim());

                if l_val == r_val || b_val == Some(r_val) {
                    out_entries.push(l_ent.clone());
                } else if b_val == Some(l_val) {
                    let mut updated = l_ent.clone();
                    updated.value = r_ent.value.clone();
                    updated.edited = true;
                    out_entries.push(updated);
                } else {
                    let (merged_val, sub_clean) =
                        merge_edit_value(be.map(|e| &e.value), &l_ent.value, &r_ent.value);
                    clean &= sub_clean;
                    let mut updated = l_ent.clone();
                    updated.value = merged_val;
                    updated.edited = true;
                    out_entries.push(updated);
                }
            }
        }
    }

    for (idx, entry) in out_entries.iter_mut().enumerate() {
        if idx == 0 {
            if !entry.leading_sep.is_empty()
                && !entry.leading_sep.contains("//")
                && !entry.leading_sep.contains("/*")
            {
                entry.leading_sep.clear();
            }
        } else if entry.leading_sep.is_empty() {
            entry.leading_sep = sep_lead.clone();
        }
    }

    Some((
        AttrSlot {
            raw: String::new(),
            open_delim: l.open_delim.clone(),
            close_delim: l.close_delim.clone(),
            edited: true,
            leading_trivia,
            trailing_trivia,
            entries: out_entries,
        },
        clean,
    ))
}

fn merge_edit_value(
    base: Option<&EditValue>,
    local: &EditValue,
    remote: &EditValue,
) -> (EditValue, bool) {
    let l_trim = local.raw.trim();
    let r_trim = remote.raw.trim();
    let b_trim = base.map(|b| b.raw.trim());

    if l_trim == r_trim || b_trim == Some(r_trim) {
        return (local.clone(), true);
    }
    if b_trim == Some(l_trim) {
        return (remote.clone(), true);
    }

    if let (EditValueKind::Map(l_slot), EditValueKind::Map(r_slot)) = (&local.kind, &remote.kind) {
        let empty_slot = AttrSlot::new_empty("{", "}");
        let b_slot = match base.map(|b| &b.kind) {
            Some(EditValueKind::Map(s)) => s,
            _ => &empty_slot,
        };
        if let Some((merged_slot, clean)) = merge_attr_slot(b_slot, l_slot, r_slot) {
            return (
                EditValue {
                    raw: merged_slot.export(),
                    edited: true,
                    kind: EditValueKind::Map(merged_slot),
                },
                clean,
            );
        }
    }

    (
        parse_edit_value_str(&format!(
            "@conflict(a: {}, b: {})",
            value_side_for_marker(l_trim),
            value_side_for_marker(r_trim)
        )),
        false,
    )
}
