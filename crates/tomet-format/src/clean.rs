//! Whitespace-hygiene formatting preserving verbatim raw spans (fenced code
//! blocks, and any `(content:raw)` element).

use tomet_ast::{Block, Document, ElementView, Value};
use tomet_ast_parser::parse_document;
use tomet_tree::{Node, Place, Visit, walk};

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

pub(crate) fn is_raw_element(el: ElementView<'_>) -> bool {
    if el.name.is_bare_named("raw") {
        return true;
    }
    if let Some(Value::Map(entries)) = el.data.args.as_ref()
        && entries
            .iter()
            .any(|(k, v)| k == "content" && matches!(v, Value::String(s) if s == "raw"))
    {
        return true;
    }
    false
}

/// The spans of the text inside every raw element of `doc`, an element written as `-@raw ...` in
/// a list item included. A raw element can sit anywhere an element can: in a `:connect` too, and
/// in a hole's content. What is in `(args)` or is the value of a `key: value` entry is not text of
/// the document, and is not looked at.
pub(crate) fn collect_raw_spans(doc: &Document, out: &mut Vec<(usize, usize)>) {
    fn push_spans(content: Option<&[Block]>, out: &mut Vec<(usize, usize)>) {
        for block in content.into_iter().flatten() {
            let Block::Paragraph(p) = block else { continue };
            for inline in &p.content {
                let span = inline.span();
                if span.start < span.end {
                    out.push((span.start, span.end));
                }
            }
        }
    }

    let _ = walk(doc, &mut |node, place| {
        if matches!(place, Place::Args | Place::PairValue) {
            return Visit::<()>::Skip;
        }
        match node {
            // An item in the combine notation is the element it names.
            Node::Item(item) => {
                if let Some(el) = item.as_element_view()
                    && is_raw_element(el)
                {
                    push_spans(el.content, out);
                }
            }
            other => {
                if let Some(el) = other.element()
                    && is_raw_element(el.as_view())
                {
                    push_spans(el.content.as_deref(), out);
                }
            }
        }
        Visit::Descend
    });
}
