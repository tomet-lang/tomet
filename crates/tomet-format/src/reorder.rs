//! Element group order formatting according to `GroupOrder` settings.

use tomet_ast::{Block, Element, Inline, List, Sigil};
use tomet_config::{GroupOrder, PrinterConfig};
use tomet_parser::parse_document;

/// Byte offset of the `close` matching the `open` at `open_pos` in
/// `src` (which must be `open`), tracking nested `open`/`close` depth
/// and skipping over `"..."`/`'...'` quoted runs.
fn find_matching(src: &str, open_pos: usize, open: char, close: char) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = open_pos + open.len_utf8();
    let mut depth: u32 = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && c == b'"' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                let closed = bytes[i] == c;
                i += 1;
                if closed {
                    break;
                }
            }
        } else if c == open as u8 {
            depth += 1;
            i += 1;
        } else if c == close as u8 {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Formats element group order according to `config` (`PrinterConfig::group_order` / `PrinterConfig::link_group_order`).
///
/// Swaps `()` and `[]` for elements carrying both args and content groups based on configured element rules.
pub fn format_element_group_order(src: &str, config: &PrinterConfig) -> String {
    const MAX_SWAPS: usize = 200;
    let mut swaps = Vec::new();
    collect_swaps(src, config, &mut swaps, MAX_SWAPS);

    if swaps.is_empty() {
        return src.to_string();
    }

    // Sort by span length ascending (innermost / shortest span first).
    // Because swapping does not change byte length:
    // (len(part1) + len(gap) + len(part2) == len(part2) + len(gap) + len(part1)),
    // every offset remains strictly invariant during and after each swap.
    swaps.sort_by_key(|s| s.close_second - s.open_first);

    let mut buf = src.as_bytes().to_vec();

    for target in swaps {
        let part1 = buf[target.open_first..target.gap_start].to_vec();
        let gap = buf[target.gap_start..target.gap_end].to_vec();
        let part2 = buf[target.gap_end..=target.close_second].to_vec();

        let mut offset = target.open_first;
        buf[offset..offset + part2.len()].copy_from_slice(&part2);
        offset += part2.len();
        buf[offset..offset + gap.len()].copy_from_slice(&gap);
        offset += gap.len();
        buf[offset..offset + part1.len()].copy_from_slice(&part1);
    }

    String::from_utf8(buf).unwrap_or_else(|_| src.to_string())
}

#[derive(Debug, Clone, Copy)]
struct SwapTarget {
    open_first: usize,
    gap_start: usize,
    gap_end: usize,
    close_second: usize,
}

fn collect_swaps(src: &str, config: &PrinterConfig, swaps: &mut Vec<SwapTarget>, limit: usize) {
    let Ok(doc) = parse_document(src) else { return };
    for block in &doc.blocks {
        collect_swaps_in_block(block, src, config, swaps, limit);
        if swaps.len() >= limit {
            break;
        }
    }
}

fn collect_swaps_in_block(
    block: &Block,
    src: &str,
    config: &PrinterConfig,
    swaps: &mut Vec<SwapTarget>,
    limit: usize,
) {
    if swaps.len() >= limit {
        return;
    }
    match block {
        Block::Paragraph(p) => {
            for inline in &p.content {
                collect_swaps_in_inline(inline, src, config, swaps, limit);
                if swaps.len() >= limit {
                    return;
                }
            }
        }
        Block::Element(el) => {
            collect_swaps_in_element(el, src, config, swaps, limit);
        }
        Block::Section(sec) => {
            for inline in &sec.title {
                collect_swaps_in_inline(inline, src, config, swaps, limit);
                if swaps.len() >= limit {
                    return;
                }
            }
            for conn in &sec.connects {
                collect_swaps_in_element(conn, src, config, swaps, limit);
                if swaps.len() >= limit {
                    return;
                }
            }
            for child in &sec.blocks {
                collect_swaps_in_block(child, src, config, swaps, limit);
                if swaps.len() >= limit {
                    return;
                }
            }
        }
        Block::List(list) => collect_swaps_in_list(list, src, config, swaps, limit),
    }
}

fn collect_swaps_in_list(
    list: &List,
    src: &str,
    config: &PrinterConfig,
    swaps: &mut Vec<SwapTarget>,
    limit: usize,
) {
    for item in &list.items {
        collect_swaps_in_element(&item.element, src, config, swaps, limit);
        if swaps.len() >= limit {
            return;
        }
        if let Some(sub) = &item.sublist {
            collect_swaps_in_list(sub, src, config, swaps, limit);
            if swaps.len() >= limit {
                return;
            }
        }
    }
}

fn collect_swaps_in_inline(
    inline: &Inline,
    src: &str,
    config: &PrinterConfig,
    swaps: &mut Vec<SwapTarget>,
    limit: usize,
) {
    if let Inline::Element(el) = inline {
        collect_swaps_in_element(el, src, config, swaps, limit);
    }
}

fn collect_swaps_in_element(
    el: &Element,
    src: &str,
    config: &PrinterConfig,
    swaps: &mut Vec<SwapTarget>,
    limit: usize,
) {
    if swaps.len() >= limit {
        return;
    }
    // 1. Check if this element itself needs swapping
    let elem_name = el.sigil.name().map(|n| n.name.as_str()).unwrap_or("");
    if let Some(order) = config.element_group_order(elem_name)
        && matches!(el.sigil, Sigil::Named(_) | Sigil::Caret(_))
        && el.args.is_some()
        && el.content.is_some()
        && !el.span.is_dummy()
    {
        let start = el.span.start;
        let end = el.span.end;
        if start < end
            && end <= src.len()
            && let Some(target) = detect_swap_element_groups(src, start, end, order)
        {
            swaps.push(target);
            if swaps.len() >= limit {
                return;
            }
        }
    }

    // 2. Otherwise recursively check children
    if let Some(blocks) = &el.content {
        for block in blocks {
            collect_swaps_in_block(block, src, config, swaps, limit);
            if swaps.len() >= limit {
                return;
            }
        }
    }
    if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
        for child in children {
            collect_swaps_in_element(child, src, config, swaps, limit);
            if swaps.len() >= limit {
                return;
            }
        }
    }
    for conn in &el.connects {
        collect_swaps_in_element(conn, src, config, swaps, limit);
        if swaps.len() >= limit {
            return;
        }
    }
}

fn detect_swap_element_groups(
    src: &str,
    start: usize,
    end: usize,
    order: GroupOrder,
) -> Option<SwapTarget> {
    let bytes = src.as_bytes();
    let mut pos = start;

    // Skip sigil '@' or '^'
    if pos < end && (bytes[pos] == b'@' || bytes[pos] == b'^') {
        pos += 1;
        if pos < end && bytes[pos] == b'[' {
            let close = find_matching(src, pos, '[', ']')?;
            pos = close + 1;
        } else {
            while pos < end
                && (bytes[pos].is_ascii_alphanumeric() || matches!(bytes[pos], b'_' | b'-' | b'.'))
            {
                pos += 1;
            }
        }
    } else {
        return None;
    }

    // Skip whitespace between name and first group
    while pos < end && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
        pos += 1;
    }
    if pos >= end {
        return None;
    }

    if bytes[pos] == b'(' {
        let open_paren = pos;
        let close_paren = find_matching(src, open_paren, '(', ')')?;
        let mut next_pos = close_paren + 1;
        while next_pos < end && matches!(bytes[next_pos], b' ' | b'\t' | b'\n' | b'\r') {
            next_pos += 1;
        }
        if next_pos < end && bytes[next_pos] == b'[' {
            let open_bracket = next_pos;
            let close_bracket = find_matching(src, open_bracket, '[', ']')?;

            if order == GroupOrder::ContentFirst {
                return Some(SwapTarget {
                    open_first: open_paren,
                    gap_start: close_paren + 1,
                    gap_end: open_bracket,
                    close_second: close_bracket,
                });
            }
        }
    } else if bytes[pos] == b'[' {
        let open_bracket = pos;
        let close_bracket = find_matching(src, open_bracket, '[', ']')?;
        let mut next_pos = close_bracket + 1;
        while next_pos < end && matches!(bytes[next_pos], b' ' | b'\t' | b'\n' | b'\r') {
            next_pos += 1;
        }
        if next_pos < end && bytes[next_pos] == b'(' {
            let open_paren = next_pos;
            let close_paren = find_matching(src, open_paren, '(', ')')?;

            if order == GroupOrder::ArgsFirst {
                return Some(SwapTarget {
                    open_first: open_bracket,
                    gap_start: close_bracket + 1,
                    gap_end: open_paren,
                    close_second: close_paren,
                });
            }
        }
    }

    None
}
