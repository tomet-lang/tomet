//! Formatting for section headings:
//! - Title style (`section.title`: `bracketed` | `bare`)
//! - Space inside brackets (`section.space_inside_brackets`)
//! - Blank lines around sections (`section.blank_before`, `section.blank_after`)

use tomet_config::PrinterConfig;
use tomet_cst::{SyntaxKind as K, SyntaxNode};
use tomet_cst_parser::parse_cst;
use tomet_style::{BracketSpacing, SectionTitleStyle};

use crate::edit::{Edit, apply_keeping_the_document};

/// Formats the title style of section headings (`section.title`: `bracketed` vs `bare`).
///
/// When `bracketed`, headings written bare (`=== title`) are formatted between brackets (`===[ title ]`).
/// When `bare`, headings written between brackets (`===[ title ]`) are formatted bare (`=== title`).
/// Headings that cannot be safely converted (e.g. multiline titles, empty titles, or titles
/// beginning with punctuation that would alter parsing) are left as written.
pub fn format_section_titles(src: &str, config: &PrinterConfig) -> String {
    let Some(style) = config.format.section_title else {
        return src.to_string();
    };

    let root = parse_cst(src);
    let edits: Vec<Edit> = root
        .descendants()
        .filter(|node| node.kind() == K::SECTION_HEADING)
        .filter_map(|heading| title_edit(&heading, style, config))
        .collect();

    apply(src, edits)
}

fn title_edit(
    heading: &SyntaxNode,
    style: SectionTitleStyle,
    config: &PrinterConfig,
) -> Option<Edit> {
    match style {
        SectionTitleStyle::Bare => to_bare_edit(heading),
        SectionTitleStyle::Bracketed => to_bracketed_edit(heading, config),
    }
}

fn to_bare_edit(heading: &SyntaxNode) -> Option<Edit> {
    let content = heading
        .children()
        .find(|child| child.kind() == K::CONTENT)?;
    let text = content.text().to_string();
    let title = text.strip_prefix('[')?.strip_suffix(']')?;
    let trimmed = title.trim();

    // Bare titles cannot span multiple lines, be empty, or start with delimiters/characters
    // that would change CST/AST classification (such as `[`, `(`, `{`, `#`, `@`).
    if title.contains('\n') || trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(['[', '(', '{', '#', '@']) || trimmed.contains("::") {
        return None;
    }

    // Leading `=` run in heading
    let equals_len = heading
        .children_with_tokens()
        .take_while(|e| e.kind() == K::EQUAL)
        .count();
    if equals_len == 0 {
        return None;
    }
    let h_start = usize::from(heading.text_range().start());
    let equals_end = h_start + equals_len;

    let c_end = usize::from(content.text_range().end());
    let replacement = format!(" {trimmed}");

    let current = &heading.text().to_string()[equals_len..(c_end - h_start)];
    (current != replacement).then(|| (equals_end, c_end, replacement))
}

fn to_bracketed_edit(heading: &SyntaxNode, config: &PrinterConfig) -> Option<Edit> {
    let body = heading.children().find(|child| child.kind() == K::BODY)?;
    let text = body.text().to_string();
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return None;
    }

    // If there were decorative trailing `=`, strip them from the inner title
    let title = trimmed.trim_end_matches('=').trim();
    if title.is_empty() {
        return None;
    }

    let spacing = config
        .format
        .section_brackets
        .unwrap_or(BracketSpacing::Padded);
    let wrapped = spacing.wrap(title);

    let equals_len = heading
        .children_with_tokens()
        .take_while(|e| e.kind() == K::EQUAL)
        .count();
    if equals_len == 0 {
        return None;
    }
    let h_start = usize::from(heading.text_range().start());
    let equals_end = h_start + equals_len;

    let trailing_ws = text.len() - text.trim_end().len();
    let b_end = usize::from(body.text_range().end()) - trailing_ws;
    let current = &heading.text().to_string()[equals_len..(b_end - h_start)];

    (current != wrapped).then(|| (equals_end, b_end, wrapped))
}

/// Spaces the title of each section heading written between brackets (`=[ title ]`) the way
/// `section.space_inside_brackets` asks.
pub fn format_section_brackets(src: &str, config: &PrinterConfig) -> String {
    let Some(spacing) = config.format.section_brackets else {
        return src.to_string();
    };
    let root = parse_cst(src);
    let edits: Vec<Edit> = root
        .descendants()
        .filter(|node| node.kind() == K::SECTION_HEADING)
        .filter_map(|heading| bracket_edit(&heading, spacing))
        .collect();
    apply(src, edits)
}

fn bracket_edit(heading: &SyntaxNode, spacing: BracketSpacing) -> Option<Edit> {
    let content = heading
        .children()
        .find(|child| child.kind() == K::CONTENT)?;
    let text = content.text().to_string();
    let title = text.strip_prefix('[')?.strip_suffix(']')?;
    if title.contains('\n') || title.trim().is_empty() {
        return None;
    }
    let spaced = spacing.wrap(title.trim());
    let range = content.text_range();
    (spaced != text).then(|| (usize::from(range.start()), usize::from(range.end()), spaced))
}

/// Formats blank lines before and after headings according to configuration.
pub fn format_section_blank_lines(src: &str, config: &PrinterConfig) -> String {
    let before_opt = config.format.section_blank_before;
    let after_opt = config.format.section_blank_after;
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
                    if a <= b
                        && let Some(edit) = make_gap_edit(src, a, b, want_blank)
                    {
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
                    // Precedence: section.blank_before if set, else section.blank_after.
                    let want_blank_opt = before_opt.or(after_opt);
                    if let Some(want_blank) = want_blank_opt {
                        let a = h.end;
                        let b = ranges[i + 1].start;
                        if a <= b
                            && let Some(edit) = make_gap_edit(src, a, b, want_blank)
                        {
                            edits.push(edit);
                        }
                    }
                } else {
                    // Gap between this heading and regular body content.
                    if let Some(want_blank) = after_opt {
                        let a = h.end;
                        let b = find_line_start(src, next_non_ws);
                        if a <= b
                            && let Some(edit) = make_gap_edit(src, a, b, want_blank)
                        {
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

    edits.sort_by_key(|e| e.0);
    apply(src, edits)
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
                break;
            }
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
            break;
        }
    }

    let glue_edit = if let (Some(c_end), true) = (comment_end, has_blank_before_heading) {
        Some((c_end + 1, h_start, String::new()))
    } else {
        None
    };

    (current_start, glue_edit)
}

fn apply(src: &str, edits: Vec<Edit>) -> String {
    if edits.is_empty() {
        return src.to_string();
    }
    apply_keeping_the_document(src, edits).unwrap_or_else(|| src.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format_blanks(before: Option<bool>, after: Option<bool>, src: &str) -> String {
        let mut config = PrinterConfig::default();
        config.format.section_blank_before = before;
        config.format.section_blank_after = after;
        format_section_blank_lines(src, &config)
    }

    #[test]
    fn section_title_bracketed_to_bare() {
        let mut cfg = PrinterConfig::default();
        cfg.format.section_title = Some(SectionTitleStyle::Bare);

        assert_eq!(format_section_titles("=[ Title ]\n", &cfg), "= Title\n");
        assert_eq!(format_section_titles("==[ Title ]\n", &cfg), "== Title\n");
        assert_eq!(
            format_section_titles("===[ Title ] {a: 1}\n", &cfg),
            "=== Title {a: 1}\n"
        );
        assert_eq!(
            format_section_titles("===[ Title ] #(id)\n", &cfg),
            "=== Title #(id)\n"
        );
    }

    #[test]
    fn section_title_bare_to_bracketed() {
        let mut cfg = PrinterConfig::default();
        cfg.format.section_title = Some(SectionTitleStyle::Bracketed);

        assert_eq!(format_section_titles("= Title\n", &cfg), "=[ Title ]\n");
        assert_eq!(format_section_titles("== Title\n", &cfg), "==[ Title ]\n");
        assert_eq!(
            format_section_titles("=== Title {a: 1}\n", &cfg),
            "===[ Title ] {a: 1}\n"
        );

        // Respects section_brackets if tight
        cfg.format.section_brackets = Some(BracketSpacing::Tight);
        assert_eq!(format_section_titles("= Title\n", &cfg), "=[Title]\n");
    }

    #[test]
    fn blank_before_and_after_section() {
        let tight = "para\n=[ H ]\nbody\n";
        let spaced = "para\n\n=[ H ]\n\nbody\n";

        assert_eq!(format_blanks(Some(true), Some(true), tight), spaced);
        assert_eq!(format_blanks(Some(false), Some(false), spaced), tight);
    }
}
