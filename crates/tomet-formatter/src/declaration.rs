//! Blank line formatting around preamble declarations according to
//! `declaration.blank_between` and `declaration.blank_after`.

use tomet_config::PrinterConfig;
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_cst_parser::parse_cst;

use crate::edit::{Edit, apply_keeping_the_document};

/// Formats blank lines around declarations according to configuration.
pub fn format_declaration_blank_lines(src: &str, config: &PrinterConfig) -> String {
    let between_opt = config.format.declaration_blank_between;
    let after_opt = config.format.declaration_blank_after;
    if between_opt.is_none() && after_opt.is_none() {
        return src.to_string();
    }

    let root = parse_cst(src);
    let mut declarations = Vec::new();

    for child in root.children_with_tokens() {
        match child {
            SyntaxElement::Token(t) if t.kind().is_trivia() || t.kind() == K::SEMI => {}
            SyntaxElement::Node(n) if is_declaration(&n) => {
                declarations.push(n);
            }
            SyntaxElement::Node(_) => {
                // Content started; declarations must be at the top of the file.
                break;
            }
            _ => break,
        }
    }

    if declarations.is_empty() {
        return src.to_string();
    }

    let mut ranges = Vec::new();
    let mut comment_glue_edits = Vec::new();

    for decl in &declarations {
        let d_start = usize::from(decl.text_range().start());
        let d_end = usize::from(decl.text_range().end());
        let line_end = find_line_end(src, d_end);

        let (start, glue_edit) = find_leading_comments(src, d_start);
        if let Some(edit) = glue_edit {
            comment_glue_edits.push(edit);
        }

        ranges.push(DeclRange {
            start,
            end: line_end,
        });
    }

    let mut edits = Vec::new();

    // 1. Blank lines between declarations (`declaration.blank_between`)
    if let Some(want_blank) = between_opt {
        for i in 0..ranges.len().saturating_sub(1) {
            let a = ranges[i].end + 1;
            let b = ranges[i + 1].start;
            if a <= b && let Some(edit) = make_gap_edit(src, a, b, want_blank) {
                edits.push(edit);
            }
        }
    }

    // 2. Blank line after the preamble declarations (`declaration.blank_after`)
    if let Some(want_blank) = after_opt
        && let Some(last) = ranges.last()
    {
        let a = last.end + 1;
        if let Some(next_non_ws) = find_next_non_whitespace(src, a) {
            // If the next non-whitespace is a heading and heading.blank_before is explicitly configured,
            // heading.blank_before takes precedence; skip it here.
            let next_is_heading = is_heading_at(src, next_non_ws, &root);
            let heading_before_configured = config.format.heading_blank_before.is_some();

            if !(next_is_heading && heading_before_configured) {
                let b = find_content_start(src, next_non_ws);
                if a <= b && let Some(edit) = make_gap_edit(src, a, b, want_blank) {
                    edits.push(edit);
                }
            }
        }
    }

    edits.extend(comment_glue_edits);

    if edits.is_empty() {
        return src.to_string();
    }

    edits.sort_by_key(|e| e.0);
    apply_keeping_the_document(src, edits).unwrap_or_else(|| src.to_string())
}

struct DeclRange {
    start: usize,
    end: usize,
}

fn is_declaration(node: &SyntaxNode) -> bool {
    node.kind() == K::BLOCK_FORM
        && node
            .children()
            .find(|n| n.kind() == K::SIGIL)
            .and_then(|n| n.first_token())
            .is_some_and(|t| t.kind() == K::COLON)
}

fn is_heading_at(src: &str, pos: usize, root: &SyntaxNode) -> bool {
    root.descendants()
        .filter(|n| n.kind() == K::SECTION_HEADING)
        .any(|h| {
            let h_start = usize::from(h.text_range().start());
            let (start, _) = find_leading_comments(src, h_start);
            pos >= start && pos <= usize::from(h.text_range().end())
        })
}

fn find_content_start(src: &str, next_non_ws: usize) -> usize {
    let root = parse_cst(src);
    // If the next content is a heading, start at its leading comments if any.
    for h in root.descendants().filter(|n| n.kind() == K::SECTION_HEADING) {
        let h_start = usize::from(h.text_range().start());
        let (start, _) = find_leading_comments(src, h_start);
        if next_non_ws >= start && next_non_ws <= usize::from(h.text_range().end()) {
            return start;
        }
    }
    find_line_start(src, next_non_ws)
}

fn make_gap_edit(src: &str, a: usize, b: usize, want_blank: bool) -> Option<Edit> {
    let gap = &src[a..b];
    let has_blank = gap.contains('\n');

    if want_blank && !has_blank {
        Some((a, b, "\n".to_string()))
    } else if !want_blank && has_blank {
        Some((a, b, String::new()))
    } else {
        None
    }
}

fn find_line_end(src: &str, idx: usize) -> usize {
    src[idx..].find('\n').map_or(src.len(), |i| idx + i)
}

fn find_line_start(src: &str, idx: usize) -> usize {
    src[..idx].rfind('\n').map_or(0, |i| i + 1)
}

fn find_next_non_whitespace(src: &str, idx: usize) -> Option<usize> {
    src[idx..]
        .find(|c: char| !c.is_whitespace())
        .map(|i| idx + i)
}

fn find_leading_comments(src: &str, h_start: usize) -> (usize, Option<Edit>) {
    let mut current_start = h_start;
    let mut search_pos = h_start;
    let mut has_blank_before = false;
    let mut comment_end = None;

    while search_pos > 0 {
        let line_end = match src[..search_pos].rfind('\n') {
            Some(i) => i,
            None => break,
        };
        let line_start = src[..line_end].rfind('\n').map_or(0, |i| i + 1);
        let line = &src[line_start..line_end];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if comment_end.is_some() {
                break;
            }
            has_blank_before = true;
            search_pos = line_start;
            continue;
        }

        if trimmed.starts_with("//") || (trimmed.starts_with("/*") && trimmed.ends_with("*/")) {
            if comment_end.is_none() {
                comment_end = Some(line_end);
            }
            current_start = line_start;
            search_pos = line_start;
        } else {
            break;
        }
    }

    let glue_edit = if let (Some(c_end), true) = (comment_end, has_blank_before) {
        Some((c_end + 1, h_start, String::new()))
    } else {
        None
    };

    (current_start, glue_edit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_with(between: Option<bool>, after: Option<bool>, src: &str) -> String {
        let mut config = PrinterConfig::default();
        config.format.declaration_blank_between = between;
        config.format.declaration_blank_after = after;
        format_declaration_blank_lines(src, &config)
    }

    #[test]
    fn unset_options_leave_source_untouched() {
        let src = "::kind(fixture)\n::meta(x: 1)\n\ntext\n";
        assert_eq!(format_with(None, None, src), src);
    }

    #[test]
    fn blank_between_inserts_and_removes_blank_line() {
        let tight = "::kind(fixture)\n::meta(x: 1)\n\ntext\n";
        let spaced = "::kind(fixture)\n\n::meta(x: 1)\n\ntext\n";

        assert_eq!(format_with(Some(true), None, tight), spaced);
        assert_eq!(format_with(Some(true), None, spaced), spaced);

        assert_eq!(format_with(Some(false), None, spaced), tight);
        assert_eq!(format_with(Some(false), None, tight), tight);
    }

    #[test]
    fn blank_after_inserts_and_removes_blank_line() {
        let tight = "::kind(fixture)\ntext\n";
        let spaced = "::kind(fixture)\n\ntext\n";

        assert_eq!(format_with(None, Some(true), tight), spaced);
        assert_eq!(format_with(None, Some(true), spaced), spaced);

        assert_eq!(format_with(None, Some(false), spaced), tight);
        assert_eq!(format_with(None, Some(false), tight), tight);
    }

    #[test]
    fn blank_after_with_heading_precedence() {
        let tight = "::kind(fixture)\n=[ Title ]\n";
        let spaced = "::kind(fixture)\n\n=[ Title ]\n";

        // When heading.blank_before is Some, it takes precedence (declaration.blank_after is ignored).
        let mut cfg = PrinterConfig::default();
        cfg.format.heading_blank_before = Some(true);
        cfg.format.declaration_blank_after = Some(false);
        // format_declaration_blank_lines skips because heading_blank_before is configured
        assert_eq!(format_declaration_blank_lines(tight, &cfg), tight);

        // When heading.blank_before is None, declaration.blank_after applies.
        assert_eq!(format_with(None, Some(true), tight), spaced);
        assert_eq!(format_with(None, Some(false), spaced), tight);
    }

    #[test]
    fn declaration_formatting_is_idempotent() {
        let src = "::kind(fixture)\n\n::meta(x: 1)\ntext\n";
        for between in [None, Some(true), Some(false)] {
            for after in [None, Some(true), Some(false)] {
                let once = format_with(between, after, src);
                let twice = format_with(between, after, &once);
                assert_eq!(once, twice, "idempotent for between={between:?}, after={after:?}");
            }
        }
    }
}
