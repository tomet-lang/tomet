//! Whitespace-hygiene formatter for `.tm` source text.
//!
//! Upgraded to be AST-aware using source [`typedmark_ast::Span`] metadata.
//! Normalizes whitespace policy (LF line endings, no trailing whitespace,
//! collapsed excess blank lines, exactly one final newline) while losslessly
//! preserving literal whitespace and blank lines inside raw/verbatim content
//! (`<codeblock>[...]` and elements opting in via `content:raw`).

use typedmark_ast::{Block, Document, Element, ElementValue, Inline, Sigil, Value};
use typedmark_parser::parse_document;

/// Format `src` in place (returns a new `String`). Idempotent:
/// `format_source(&format_source(src)) == format_source(src)`.
pub fn format_source(src: &str) -> String {
    let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.trim().is_empty() {
        return String::new();
    }

    let mut raw_spans = Vec::new();
    if let Ok(doc) = parse_document(&normalized) {
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

fn is_raw_element(el: &Element) -> bool {
    if matches!(&el.sigil, Sigil::Type(name) if name == "codeblock") {
        return true;
    }
    if let Some(Value::Map(entries)) = &el.args {
        if entries
            .iter()
            .any(|(k, v)| k == "content" && matches!(v, Value::String(s) if s == "raw"))
        {
            return true;
        }
    }
    false
}

fn collect_raw_spans(doc: &Document, out: &mut Vec<(usize, usize)>) {
    fn walk_element(el: &Element, out: &mut Vec<(usize, usize)>) {
        if is_raw_element(el) {
            if let Some(inlines) = &el.content {
                for inline in inlines {
                    let span = inline.span();
                    if span.start.offset < span.end.offset {
                        out.push((span.start.offset, span.end.offset));
                    }
                }
            }
        }
        if let Some(inlines) = &el.content {
            for inline in inlines {
                if let Inline::Element(child_el) = inline {
                    walk_element(child_el, out);
                }
            }
        }
        if let Some(ElementValue::Children(children)) = &el.value {
            for child in children {
                walk_element(child, out);
            }
        }
    }

    for block in &doc.blocks {
        match block {
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
            }
            Block::Heading(h) => {
                for inline in &h.content {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
            }
            Block::List(list) => {
                for item in &list.items {
                    for inline in &item.content {
                        if let Inline::Element(el) = inline {
                            walk_element(el, out);
                        }
                    }
                }
            }
            Block::Element(el) => {
                walk_element(el, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_trailing_whitespace() {
        assert_eq!(format_source("a  \nb\t\n"), "a\nb\n");
    }

    #[test]
    fn normalizes_line_endings() {
        assert_eq!(format_source("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(format_source("a\rb\r"), "a\nb\n");
    }

    #[test]
    fn drops_leading_blank_lines() {
        assert_eq!(format_source("\n\n\na\n"), "a\n");
    }

    #[test]
    fn collapses_multiple_blank_lines_to_one() {
        assert_eq!(format_source("a\n\n\n\nb\n"), "a\n\nb\n");
    }

    #[test]
    fn drops_trailing_blank_lines_and_ensures_one_final_newline() {
        assert_eq!(format_source("a\n\n\n"), "a\n");
        assert_eq!(format_source("a"), "a\n");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(format_source(""), "");
        assert_eq!(format_source("\n\n  \n"), "");
    }

    #[test]
    fn already_formatted_input_is_unchanged() {
        let src = "#[ Hello ]\n\n- one\n- two\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn raw_content_is_preserved_losslessly() {
        let src = "<memo>(content:raw)[\nline one  \n\nline two\n]\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn codeblock_content_is_preserved_losslessly() {
        let src = "<codeblock>(lang:rust)[\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n]\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn fenced_code_block_content_is_preserved_losslessly() {
        let src = "```rust\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n```\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn is_idempotent_on_repo_spec_examples() {
        for src in repo_examples() {
            let once = format_source(src);
            let twice = format_source(&once);
            assert_eq!(once, twice, "formatting is not idempotent for: {src:?}");
        }
    }

    #[test]
    fn does_not_change_the_parsed_document() {
        for src in [
            include_str!("../../../docs/readme.ja.tm"),
            include_str!("../../../docs/tmt/examples/image.meta.tm"),
            include_str!("../../../docs/develop/roadmap.ja.tm"),
        ] {
            let before = typedmark_parser::parse_document(src)
                .unwrap_or_else(|e| panic!("fixture failed to parse: {e}"));
            let formatted = format_source(src);
            let after = typedmark_parser::parse_document(&formatted)
                .unwrap_or_else(|e| panic!("formatted output failed to parse: {e}"));
            assert_eq!(before, after, "formatting changed the parsed document");
        }
    }

    fn repo_examples() -> Vec<&'static str> {
        vec![
            include_str!("../../../docs/readme.ja.tm"),
            include_str!("../../../docs/tmt/examples/image.meta.tm"),
            include_str!("../../../docs/ja/cheatsheet.tm"),
            include_str!("../../../docs/develop/roadmap.ja.tm"),
        ]
    }
}
