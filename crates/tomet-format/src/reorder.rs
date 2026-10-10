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
    let mut current = src.to_string();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 200;

    while iterations < MAX_ITERATIONS {
        if let Some(next) = find_and_swap_one_element(&current, config) {
            current = next;
            iterations += 1;
        } else {
            break;
        }
    }
    current
}

fn find_and_swap_one_element(src: &str, config: &PrinterConfig) -> Option<String> {
    let doc = parse_document(src).ok()?;
    for block in &doc.blocks {
        if let Some(swapped) = find_swap_in_block(block, src, config) {
            return Some(swapped);
        }
    }
    None
}

fn find_swap_in_block(block: &Block, src: &str, config: &PrinterConfig) -> Option<String> {
    match block {
        Block::Paragraph(p) => {
            for inline in &p.content {
                if let Some(swapped) = find_swap_in_inline(inline, src, config) {
                    return Some(swapped);
                }
            }
        }
        Block::Element(el) => {
            if let Some(swapped) = check_and_swap_element(el, src, config) {
                return Some(swapped);
            }
        }
        Block::Section(sec) => {
            for inline in &sec.title {
                if let Some(swapped) = find_swap_in_inline(inline, src, config) {
                    return Some(swapped);
                }
            }
            for conn in &sec.connects {
                if let Some(swapped) = check_and_swap_element(conn, src, config) {
                    return Some(swapped);
                }
            }
            for child in &sec.blocks {
                if let Some(swapped) = find_swap_in_block(child, src, config) {
                    return Some(swapped);
                }
            }
        }
        Block::List(list) => return find_swap_in_list(list, src, config),
    }
    None
}

fn find_swap_in_list(list: &List, src: &str, config: &PrinterConfig) -> Option<String> {
    for item in &list.items {
        if let Some(swapped) = check_and_swap_element(&item.element, src, config) {
            return Some(swapped);
        }
        if let Some(sub) = &item.sublist
            && let Some(swapped) = find_swap_in_list(sub, src, config)
        {
            return Some(swapped);
        }
    }
    None
}

fn find_swap_in_inline(inline: &Inline, src: &str, config: &PrinterConfig) -> Option<String> {
    match inline {
        Inline::Element(el) => check_and_swap_element(el, src, config),
        _ => None,
    }
}

fn check_and_swap_element(el: &Element, src: &str, config: &PrinterConfig) -> Option<String> {
    // 1. Check if this element itself needs swapping
    let elem_name = el.sigil.name().map(|n| n.name.as_str()).unwrap_or("");
    if let Some(order) = config.element_group_order(elem_name)
        && matches!(el.sigil, Sigil::Named(_) | Sigil::Caret(_))
        && el.args.is_some()
        && el.content.is_some()
        && !el.span.is_dummy()
    {
        let start = el.span.start.offset;
        let end = el.span.end.offset;
        if start < end
            && end <= src.len()
            && let Some(swapped) = try_swap_element_groups(src, start, end, order)
        {
            return Some(swapped);
        }
    }

    // 2. Otherwise recursively check children
    if let Some(blocks) = &el.content {
        for block in blocks {
            if let Some(swapped) = find_swap_in_block(block, src, config) {
                return Some(swapped);
            }
        }
    }
    if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
        for child in children {
            if let Some(swapped) = check_and_swap_element(child, src, config) {
                return Some(swapped);
            }
        }
    }
    for conn in &el.connects {
        if let Some(swapped) = check_and_swap_element(conn, src, config) {
            return Some(swapped);
        }
    }

    None
}

fn try_swap_element_groups(
    src: &str,
    start: usize,
    end: usize,
    order: GroupOrder,
) -> Option<String> {
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
                let args_part = &src[open_paren..=close_paren];
                let gap = &src[close_paren + 1..open_bracket];
                let content_part = &src[open_bracket..=close_bracket];

                let mut out = String::with_capacity(src.len());
                out.push_str(&src[..open_paren]);
                out.push_str(content_part);
                out.push_str(gap);
                out.push_str(args_part);
                out.push_str(&src[close_bracket + 1..]);
                return Some(out);
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
                let content_part = &src[open_bracket..=close_bracket];
                let gap = &src[close_bracket + 1..open_paren];
                let args_part = &src[open_paren..=close_paren];

                let mut out = String::with_capacity(src.len());
                out.push_str(&src[..open_bracket]);
                out.push_str(args_part);
                out.push_str(gap);
                out.push_str(content_part);
                out.push_str(&src[close_paren + 1..]);
                return Some(out);
            }
        }
    }

    None
}
