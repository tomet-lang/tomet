//! Whitespace-hygiene formatting preserving verbatim raw spans (fenced code
//! blocks, and any `(content:raw)` element).

use tomet_ast::{Block, Document, Element, Inline, ListItem, Value};
use tomet_parser::parse_document;

/// Normalizes whitespace (LF line endings, no trailing whitespace, collapsed excess
/// blank lines, exactly one final newline) while losslessly preserving literal whitespace
/// and blank lines inside raw/verbatim content.
pub fn clean_whitespace(normalized: &str) -> String {
    if normalized.trim().is_empty() {
        return String::new();
    }

    let mut raw_spans = Vec::new();
    if let Ok(doc) = parse_document(normalized) {
        collect_raw_spans(&doc, &mut raw_spans);
    }

    let is_offset_raw = |offset: usize| -> bool {
        raw_spans
            .iter()
            .any(|&(start, end)| offset >= start && offset < end)
    };

    let mut out_lines: Vec<String> = Vec::new();
    let mut pending_blank = false;
    let mut current_offset = 0;

    for line in normalized.split('\n') {
        let line_len = line.len();
        let line_end_offset = current_offset + line_len;

        // Check if any byte in this line falls inside raw content.
        let is_raw_line = (current_offset..=line_end_offset).any(is_offset_raw);

        if is_raw_line {
            if pending_blank && !out_lines.is_empty() {
                out_lines.push(String::new());
            }
            pending_blank = false;
            out_lines.push(line.to_string());
        } else {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.is_empty() {
                pending_blank = true;
            } else {
                if pending_blank && !out_lines.is_empty() {
                    out_lines.push(String::new());
                }
                pending_blank = false;
                out_lines.push(trimmed.to_string());
            }
        }

        // +1 accounts for the newline character in the normalized string
        current_offset += line_len + 1;
    }

    if out_lines.is_empty() {
        return String::new();
    }

    let mut out = out_lines.join("\n");
    out.push('\n');
    out
}

pub(crate) fn is_raw_element(el: &Element) -> bool {
    if el.sigil.is_bare_named("raw") {
        return true;
    }
    if let Some(Value::Map(entries)) = &el.args
        && entries
            .iter()
            .any(|(k, v)| k == "content" && matches!(v, Value::String(s) if s == "raw"))
    {
        return true;
    }
    false
}

pub(crate) fn collect_raw_spans(doc: &Document, out: &mut Vec<(usize, usize)>) {
    fn walk_element(el: &Element, out: &mut Vec<(usize, usize)>) {
        if is_raw_element(el)
            && let Some(blocks) = &el.content
        {
            for block in blocks {
                let Block::Paragraph(p) = block else { continue };
                for inline in &p.content {
                    let span = inline.span();
                    if span.start.offset < span.end.offset {
                        out.push((span.start.offset, span.end.offset));
                    }
                }
            }
        }
        if let Some(blocks) = &el.content {
            for block in blocks {
                walk_block(block, out);
            }
        }
        if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
            for child in children {
                walk_element(child, out);
            }
        }
    }

    fn walk_block(block: &Block, out: &mut Vec<(usize, usize)>) {
        match block {
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
            }
            Block::Element(el) => walk_element(el, out),
            Block::Section(sec) => {
                for inline in &sec.title {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
                for conn in &sec.connects {
                    walk_element(conn, out);
                }
                for child in &sec.blocks {
                    walk_block(child, out);
                }
            }
            Block::List(list) => {
                for conn in &list.connects {
                    walk_element(conn, out);
                }
                walk_list_items(&list.items, out);
            }
        }
    }

    fn walk_list_items(items: &[ListItem], out: &mut Vec<(usize, usize)>) {
        for item in items {
            walk_element(&item.element, out);
            if let Some(sub) = &item.sublist {
                for conn in &sub.connects {
                    walk_element(conn, out);
                }
                walk_list_items(&sub.items, out);
            }
        }
    }

    for block in &doc.blocks {
        walk_block(block, out);
    }
}
