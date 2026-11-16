//! How the whitespace and comments between nodes are merged: [`merge_ws`] and the trivia of a run of children.

use crate::diff3;
use crate::markers::{BlockUnit, UnitKind};
use tomet_edit::{ChildItem, EditDoc, export_node_str};

enum TriviaPiece {
    Ws(String),
    Comment(String),
}

fn split_trivia_tokens(s: &str) -> Vec<TriviaPiece> {
    let mut out = Vec::new();
    let mut pos = 0;
    let mut ws_start = 0;

    while pos < s.len() {
        let rest = &s[pos..];
        if rest.starts_with("//") {
            if pos > ws_start {
                out.push(TriviaPiece::Ws(s[ws_start..pos].to_string()));
            }
            let line_end = rest.find('\n').map_or(s.len(), |i| pos + i + 1);
            out.push(TriviaPiece::Comment(s[pos..line_end].to_string()));
            pos = line_end;
            ws_start = pos;
        } else if rest.starts_with("/*") {
            if pos > ws_start {
                out.push(TriviaPiece::Ws(s[ws_start..pos].to_string()));
            }
            let comment_end = rest.find("*/").map_or(s.len(), |i| pos + i + 2);
            out.push(TriviaPiece::Comment(s[pos..comment_end].to_string()));
            pos = comment_end;
            ws_start = pos;
        } else {
            pos += rest.chars().next().map_or(1, char::len_utf8);
        }
    }

    if ws_start < s.len() {
        out.push(TriviaPiece::Ws(s[ws_start..].to_string()));
    }
    out
}

pub(crate) fn decompose_children(doc: &EditDoc, items: &[ChildItem]) -> (Vec<BlockUnit>, String) {
    let mut units: Vec<BlockUnit> = Vec::new();
    let mut pending_ws = String::new();

    for item in items {
        match item {
            ChildItem::Trivia(t) => {
                let mut slice = t.as_str();
                if pending_ws.is_empty()
                    && let Some(BlockUnit {
                        kind:
                            UnitKind::Node {
                                raw, trailing_nl, ..
                            },
                        ..
                    }) = units.last_mut()
                    && !raw.ends_with('\n')
                    && trailing_nl.is_empty()
                {
                    if let Some(rest) = slice.strip_prefix("\r\n") {
                        *trailing_nl = "\r\n".to_string();
                        slice = rest;
                    } else if let Some(rest) = slice.strip_prefix('\n') {
                        *trailing_nl = "\n".to_string();
                        slice = rest;
                    }
                }
                for piece in split_trivia_tokens(slice) {
                    match piece {
                        TriviaPiece::Ws(ws) => pending_ws.push_str(&ws),
                        TriviaPiece::Comment(c) => {
                            units.push(BlockUnit {
                                leading_ws: std::mem::take(&mut pending_ws),
                                kind: UnitKind::Comment(c),
                            });
                        }
                    }
                }
            }
            ChildItem::Node(id) => {
                let raw = export_node_str(doc, *id);
                units.push(BlockUnit {
                    leading_ws: std::mem::take(&mut pending_ws),
                    kind: UnitKind::Node {
                        id: *id,
                        raw,
                        trailing_nl: String::new(),
                    },
                });
            }
        }
    }

    (units, pending_ws)
}

pub(crate) fn merge_ws(base: &str, local: &str, remote: &str) -> String {
    if local == remote || base == remote {
        local.to_string()
    } else if base == local {
        remote.to_string()
    } else {
        local.to_string()
    }
}

pub(crate) fn propagate_matched_leading_ws(
    base: &mut [BlockUnit],
    local: &mut [BlockUnit],
    remote: &mut [BlockUnit],
) {
    let mut local_map = vec![None; base.len()];
    for (bi, li) in diff3::lcs_pairs(base, local) {
        local_map[bi] = Some(li);
    }
    let mut remote_map = vec![None; base.len()];
    for (bi, ri) in diff3::lcs_pairs(base, remote) {
        remote_map[bi] = Some(ri);
    }

    for bi in 0..base.len() {
        match (local_map[bi], remote_map[bi]) {
            (Some(li), Some(ri)) => {
                let merged = merge_ws(
                    &base[bi].leading_ws,
                    &local[li].leading_ws,
                    &remote[ri].leading_ws,
                );
                base[bi].leading_ws = merged.clone();
                local[li].leading_ws = merged.clone();
                remote[ri].leading_ws = merged;
            }
            (Some(li), None) => base[bi].leading_ws = local[li].leading_ws.clone(),
            (None, Some(ri)) => base[bi].leading_ws = remote[ri].leading_ws.clone(),
            (None, None) => {}
        }
    }
}
