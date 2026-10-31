//! Construction and detection of `@conflict` marker elements, and the
//! text/block unit helpers shared by [`crate::merge`], [`crate::resolve`],
//! and [`crate::conflict`].

use tomet_edit::{EditDoc, NodeId, import_source};

/// The raw-text substring other crates can look for without parsing a
/// document -- a cheap "does this note have a conflict in it at all"
/// check over file content read straight off disk.
pub const CONFLICT_MARKER: &str = "@conflict";

#[derive(Debug, Clone)]
pub(crate) struct BlockUnit {
    pub leading_ws: String,
    pub kind: UnitKind,
}

#[derive(Debug, Clone)]
pub(crate) enum UnitKind {
    Comment(String),
    Node {
        id: NodeId,
        raw: String,
        trailing_nl: String,
    },
}

impl PartialEq for BlockUnit {
    fn eq(&self, other: &Self) -> bool {
        match (&self.kind, &other.kind) {
            (UnitKind::Comment(a), UnitKind::Comment(b)) => a == b,
            (UnitKind::Node { raw: a, .. }, UnitKind::Node { raw: b, .. }) => a == b,
            _ => false,
        }
    }
}

pub(crate) fn render_units_raw(units: &[BlockUnit]) -> String {
    let mut s = String::new();
    for (idx, u) in units.iter().enumerate() {
        if idx > 0 {
            s.push_str(&u.leading_ws);
        }
        match &u.kind {
            UnitKind::Comment(c) => s.push_str(c),
            UnitKind::Node {
                raw, trailing_nl, ..
            } => {
                s.push_str(raw);
                s.push_str(trailing_nl);
            }
        }
    }
    s
}

pub(crate) fn format_conflict_side_blocks(units: &[BlockUnit], raw_side: &str) -> String {
    let trimmed = raw_side
        .strip_suffix("\r\n")
        .or_else(|| raw_side.strip_suffix('\n'))
        .unwrap_or(raw_side);
    let is_single_plain_para = units.len() == 1
        && !trimmed.contains('\n')
        && !trimmed.starts_with([' ', '\t', '@', ':', '=', '-', '+', '*', '`', '/'])
        && !trimmed.ends_with([' ', '\t']);

    if is_single_plain_para {
        format!("[ {trimmed} ]")
    } else {
        let mut out = String::from("[\n");
        for line in trimmed.split('\n') {
            if line.is_empty() {
                out.push('\n');
            } else {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push_str("  ]");
        out
    }
}

pub(crate) fn build_conflict_block_unit(
    mine: &[BlockUnit],
    theirs: &[BlockUnit],
    leading_ws: String,
    target_doc: &mut EditDoc,
) -> BlockUnit {
    let mine_raw = render_units_raw(mine);
    let theirs_raw = render_units_raw(theirs);
    let a_side = format_conflict_side_blocks(mine, &mine_raw);
    let b_side = format_conflict_side_blocks(theirs, &theirs_raw);

    let conflict_body = format!("@conflict(\n  a: {a_side}\n  b: {b_side}\n)");
    let conflict_src = format!("{conflict_body}\n");
    let parsed_doc = import_source(&conflict_src);
    let root_id = parsed_doc
        .root_nodes()
        .next()
        .expect("@conflict source must parse as a BlockElement node");
    let new_id = target_doc.clone_node_from(&parsed_doc, root_id);

    BlockUnit {
        leading_ws,
        kind: UnitKind::Node {
            id: new_id,
            raw: conflict_body,
            trailing_nl: "\n".to_string(),
        },
    }
}

pub(crate) fn format_inline_conflict_span(mine_raw: &str, theirs_raw: &str) -> String {
    let common_lead_len = mine_raw
        .chars()
        .zip(theirs_raw.chars())
        .take_while(|(a, b)| a == b && a.is_whitespace())
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();

    let m_after_lead = &mine_raw[common_lead_len..];
    let t_after_lead = &theirs_raw[common_lead_len..];

    let common_trail_len = m_after_lead
        .chars()
        .rev()
        .zip(t_after_lead.chars().rev())
        .take_while(|(a, b)| a == b && a.is_whitespace())
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>();

    let lead = &mine_raw[..common_lead_len];
    let m_core = &m_after_lead[..m_after_lead.len() - common_trail_len];
    let t_core = &t_after_lead[..t_after_lead.len() - common_trail_len];
    let trail = &m_after_lead[m_after_lead.len() - common_trail_len..];

    format!("{lead}@conflict(a: [ {m_core} ], b: [ {t_core} ]){trail}")
}

pub(crate) fn extract_conflict_block_side(inner_raw: &str) -> String {
    if inner_raw.contains('\n') {
        let after_open = inner_raw
            .strip_prefix("\r\n")
            .or_else(|| inner_raw.strip_prefix('\n'))
            .unwrap_or(inner_raw);
        let before_close = after_open.trim_end_matches([' ', '\t']);
        let body = before_close
            .strip_suffix("\r\n")
            .or_else(|| before_close.strip_suffix('\n'))
            .unwrap_or(before_close);

        let min_indent = body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start_matches(' ').len())
            .min()
            .unwrap_or(0);

        let mut out = String::new();
        for (i, line) in body.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            if line.len() >= min_indent && line[..min_indent].chars().all(|c| c == ' ') {
                out.push_str(&line[min_indent..]);
            } else {
                out.push_str(line.trim_start_matches(' '));
            }
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    } else {
        let trimmed = inner_raw.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            format!("{trimmed}\n")
        }
    }
}

pub(crate) fn extract_conflict_inline_side(inner_raw: &str) -> String {
    let s = inner_raw.strip_prefix(' ').unwrap_or(inner_raw);
    let s = s.strip_suffix(' ').unwrap_or(s);
    s.to_string()
}

pub(crate) fn unquote_scalar_display(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        s.to_string()
    }
}

/// A value written for one side of a value-position `@conflict(a: ..., b: ...)`.
///
/// A `|` run cannot stand there: it ends where its markers stop, so it would
/// swallow the `, b: ...` after it and the marker could not be read back. It
/// means `[ ... ]` (a `|content` is that, spelled without brackets), so it is
/// written as that. Any other value is written as it is.
pub(crate) fn value_side_for_marker(raw: &str) -> String {
    let raw = raw.trim();
    let Some(rest) = raw.strip_prefix('|') else {
        return raw.to_string();
    };
    let lines: Vec<&str> = rest
        .split('\n')
        .enumerate()
        .map(|(i, line)| {
            let line = if i == 0 {
                line
            } else {
                let line = line.trim_start();
                line.strip_prefix('|').unwrap_or(line)
            };
            line.strip_prefix(' ').unwrap_or(line).trim_end()
        })
        .collect();
    match lines.as_slice() {
        [one] => format!("[ {one} ]"),
        many => {
            let mut out = String::from("[\n");
            for line in many {
                if !line.is_empty() {
                    out.push_str("    ");
                    out.push_str(line);
                }
                out.push('\n');
            }
            out.push_str("  ]");
            out
        }
    }
}
