//! Blank line formatting around section headings according to
//! `heading.blank_before` and `heading.blank_after`.

use tomet_config::PrinterConfig;
use tomet_cst::{SyntaxKind as K, SyntaxNode};
use tomet_cst_parser::parse_cst;

use crate::edit::{Edit, apply_keeping_the_document};

/// Formats blank lines before and after headings according to configuration.
pub fn format_heading_blank_lines(src: &str, config: &PrinterConfig) -> String {
    let before_opt = config.format.heading_blank_before;
    let after_opt = config.format.heading_blank_after;
    if before_opt.is_none() && after_opt.is_none() {
        return src.to_string();
    }

    let root = parse_cst(src);
    let headings: Vec<SyntaxNode> = root
        .descendants()
        .filter(|node| node.kind() == K::SECTION_HEADING)
        .collect();

    if headings.is_empty() {
        return src.to_string();
    }

    let mut ranges = Vec::new();
    let mut comment_glue_edits = Vec::new();

    for heading in headings {
        let h_start = usize::from(heading.text_range().start());
        let h_end = usize::from(heading.text_range().end());

        // Find leading comments preceding the heading.
        let (start, glue_edit) = find_leading_comments(src, h_start);
        if let Some(edit) = glue_edit {
            comment_glue_edits.push(edit);
        }

        ranges.push(HeadingRange { start, end: h_end });
    }

    let mut edits = Vec::new();
    for (i, h) in ranges.iter().enumerate() {
        // --- Before gap ---
        if let Some(prev_non_ws) = find_prev_non_whitespace(src, h.start) {
            let is_container_open = src.as_bytes()[prev_non_ws] == b'[';
            let is_prev_heading = i > 0 && prev_non_ws < ranges[i - 1].end;

            if !is_container_open && !is_prev_heading {
                // Gap between previous content and this heading.
                if let Some(want_blank) = before_opt {
                    let prev_line_end = find_line_end(src, prev_non_ws);
                    let a = prev_line_end + 1;
                    let b = h.start;
                    if a <= b && let Some(edit) = make_gap_edit(src, a, b, want_blank) {
                        edits.push(edit);
                    }
                }
            }
        }

        // --- After gap ---
        if let Some(next_non_ws) = find_next_non_whitespace(src, h.end) {
            let is_container_close = src.as_bytes()[next_non_ws] == b']';
            let is_next_heading = i + 1 < ranges.len() && next_non_ws >= ranges[i + 1].start;

            if !is_container_close {
                if is_next_heading {
                    // Consecutive headings gap.
                    // Precedence: heading.blank_before if set, else heading.blank_after.
                    let want_blank_opt = before_opt.or(after_opt);
                    if let Some(want_blank) = want_blank_opt {
                        let a = h.end;
                        let b = ranges[i + 1].start;
                        if a <= b && let Some(edit) = make_gap_edit(src, a, b, want_blank) {
                            edits.push(edit);
                        }
                    }
                } else {
                    // Gap between this heading and regular body content.
                    if let Some(want_blank) = after_opt {
                        let a = h.end;
                        let b = find_line_start(src, next_non_ws);
                        if a <= b && let Some(edit) = make_gap_edit(src, a, b, want_blank) {
                            edits.push(edit);
                        }
                    }
                }
            }
        }
    }

    edits.extend(comment_glue_edits);

    if edits.is_empty() {
        return src.to_string();
    }

    // Sort edits by start position ascending (apply_keeping_the_document applies in rev order).
    edits.sort_by_key(|e| e.0);
    apply_keeping_the_document(src, edits).unwrap_or_else(|| src.to_string())
}

struct HeadingRange {
    start: usize,
    end: usize,
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

fn find_prev_non_whitespace(src: &str, idx: usize) -> Option<usize> {
    src[..idx].rfind(|c: char| !c.is_whitespace())
}

fn find_next_non_whitespace(src: &str, idx: usize) -> Option<usize> {
    src[idx..]
        .find(|c: char| !c.is_whitespace())
        .map(|i| idx + i)
}

fn find_leading_comments(src: &str, h_start: usize) -> (usize, Option<Edit>) {
    let mut current_start = h_start;
    let mut search_pos = h_start;
    let mut has_blank_before_heading = false;
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
                // We already found comments, and now hit a blank line above them.
                break;
            }
            // Blank line between heading and comments.
            has_blank_before_heading = true;
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
            // Regular content line, stop.
            break;
        }
    }

    let glue_edit = if let (Some(c_end), true) = (comment_end, has_blank_before_heading) {
        // Glue comment and heading together by removing the blank line between them.
        Some((c_end + 1, h_start, String::new()))
    } else {
        None
    };

    (current_start, glue_edit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_with(before: Option<bool>, after: Option<bool>, src: &str) -> String {
        let mut config = PrinterConfig::default();
        config.format.heading_blank_before = before;
        config.format.heading_blank_after = after;
        format_heading_blank_lines(src, &config)
    }

    #[test]
    fn unset_options_leave_source_untouched() {
        let src = "text\n=[ A ]\ntext\n\n=[ B ]\n\ntext\n";
        assert_eq!(format_with(None, None, src), src);
    }

    #[test]
    fn blank_before_inserts_and_removes_blank_line() {
        let tight = "para\n=[ H ]\n";
        let spaced = "para\n\n=[ H ]\n";

        assert_eq!(format_with(Some(true), None, tight), spaced);
        assert_eq!(format_with(Some(true), None, spaced), spaced);

        assert_eq!(format_with(Some(false), None, spaced), tight);
        assert_eq!(format_with(Some(false), None, tight), tight);
    }

    #[test]
    fn blank_after_inserts_and_removes_blank_line() {
        let tight = "=[ H ]\npara\n";
        let spaced = "=[ H ]\n\npara\n";

        assert_eq!(format_with(None, Some(true), tight), spaced);
        assert_eq!(format_with(None, Some(true), spaced), spaced);

        assert_eq!(format_with(None, Some(false), spaced), tight);
        assert_eq!(format_with(None, Some(false), tight), tight);
    }

    #[test]
    fn file_start_and_end_headings_do_not_get_blank_lines() {
        let src = "=[ Start ]\n\n=[ End ]\n";
        let out = format_with(Some(true), Some(true), src);
        assert!(!out.starts_with('\n'));
        assert_eq!(out, "=[ Start ]\n\n=[ End ]\n");
    }

    #[test]
    fn container_start_and_end_headings_do_not_get_blank_lines() {
        let src = "@callout[\n=[ Inside ]\n]\n";
        let out = format_with(Some(true), Some(true), src);
        assert_eq!(out, src);
    }

    #[test]
    fn leading_comment_is_treated_as_part_of_heading() {
        let tight = "para\n// comment\n=[ H ]\nbody\n";
        let spaced = "para\n\n// comment\n=[ H ]\nbody\n";

        assert_eq!(format_with(Some(true), None, tight), spaced);
        assert_eq!(format_with(Some(false), None, spaced), tight);
    }

    #[test]
    fn heading_with_connect_is_treated_as_heading() {
        let tight = "=[ H ]::{ a: 1 }\nbody\n";
        let spaced = "=[ H ]::{ a: 1 }\n\nbody\n";

        assert_eq!(format_with(None, Some(true), tight), spaced);
        assert_eq!(format_with(None, Some(false), spaced), tight);
    }

    #[test]
    fn test_comment_fixture_case() {
        let src = "::kind(fixture)\n::meta(x: 1)\n\n// comment line 1\n// comment line 2\n\n=[ ブロックコメント ]\n\n@comment[ text ]\n";
        let out = format_with(Some(false), Some(true), src);
        eprintln!("ONCE:\n{out}");
        let twice = format_with(Some(false), Some(true), &out);
        eprintln!("TWICE:\n{twice}");
        assert_eq!(out, twice);
    }

    #[test]
    fn consecutive_headings_precedence() {
        let tight = "=[ A ]\n==[ B ]\n";
        let spaced = "=[ A ]\n\n==[ B ]\n";

        assert_eq!(format_with(Some(true), Some(false), tight), spaced);
        assert_eq!(format_with(Some(false), Some(true), spaced), tight);

        assert_eq!(format_with(None, Some(true), tight), spaced);
        assert_eq!(format_with(None, Some(false), tight), tight);
    }

    #[test]
    fn formatting_is_idempotent() {
        let src = "intro\n\n=[ Heading 1 ]\nbody 1\n==[ Heading 2 ]\n\nbody 2\n";
        for before in [None, Some(true), Some(false)] {
            for after in [None, Some(true), Some(false)] {
                let once = format_with(before, after, src);
                let twice = format_with(before, after, &once);
                assert_eq!(once, twice, "idempotent for before={before:?}, after={after:?}");
            }
        }
    }
}
