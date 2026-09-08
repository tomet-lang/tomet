//! Generic comment extraction engine supporting line and block comments across languages.

use std::path::Path;

use crate::lang::{CommentSyntax, Language};
use crate::{CommentKind, CommentSourceMap, ExtractedCommentBlock};

/// Options for comment extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractOptions {
    /// When true, only comments considered documentation (e.g. `///`, `//!`, `/**`, `"""`)
    /// are extracted. When false, regular comments (`//`, `#`, `/*`) are also included.
    pub doc_only: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self { doc_only: false }
    }
}

/// Extracts comments from source code based on language syntax.
pub fn extract_comments(
    source: &str,
    file: &Path,
    lang: Language,
    options: ExtractOptions,
) -> Vec<ExtractedCommentBlock> {
    let syntax = lang.syntax();
    let mut blocks = Vec::new();

    // We scan lines for line comments and block comment delimiters.
    let lines: Vec<&str> = source.lines().collect();
    let mut line_idx = 0;

    while line_idx < lines.len() {
        let line = lines[line_idx];
        let line_num = line_idx + 1;
        let trimmed_start = line.trim_start();
        let leading_spaces = line.len() - trimmed_start.len();

        // 1. Check for block comment delimiters first
        if let Some((kind, start_delim, end_delim)) = match_block_comment_start(trimmed_start, &syntax, options) {
            let start_col = leading_spaces + start_delim.len();
            let after_start = &trimmed_start[start_delim.len()..];

            // Case A: Single-line block comment, e.g. /* ... */ or """ ... """
            if let Some(end_offset) = after_start.find(end_delim) {
                let inner = &after_start[..end_offset];
                let (content, col_offset) = strip_boundary_whitespace(inner, start_col);

                blocks.push(ExtractedCommentBlock {
                    text: format!("{content}\n"),
                    kind,
                    source_map: CommentSourceMap {
                        file_path: file.to_path_buf(),
                        lines: vec![(line_num, col_offset + 1)],
                    },
                });

                line_idx += 1;
                continue;
            }

            // Case B: Multiline block comment
            let mut block_text = String::new();
            let mut source_lines = Vec::new();

            // First line content after start delimiter
            let (first_line_content, first_line_col) = strip_boundary_whitespace(after_start, start_col);
            if !first_line_content.trim().is_empty() {
                block_text.push_str(first_line_content);
                block_text.push('\n');
                source_lines.push((line_num, first_line_col + 1));
            }

            line_idx += 1;
            let mut found_end = false;

            while line_idx < lines.len() {
                let cur_line = lines[line_idx];
                let cur_line_num = line_idx + 1;

                if let Some(end_offset) = cur_line.find(end_delim) {
                    let before_end = &cur_line[..end_offset];
                    let (content, col) = clean_block_comment_line(before_end, start_delim);
                    if !content.trim().is_empty() {
                        block_text.push_str(&content);
                        block_text.push('\n');
                        source_lines.push((cur_line_num, col + 1));
                    }
                    found_end = true;
                    line_idx += 1;
                    break;
                } else {
                    let (content, col) = clean_block_comment_line(cur_line, start_delim);
                    block_text.push_str(&content);
                    block_text.push('\n');
                    source_lines.push((cur_line_num, col + 1));
                    line_idx += 1;
                }
            }

            if !block_text.is_empty() {
                blocks.push(ExtractedCommentBlock {
                    text: block_text,
                    kind,
                    source_map: CommentSourceMap {
                        file_path: file.to_path_buf(),
                        lines: source_lines,
                    },
                });
            }

            if found_end {
                continue;
            }
        }

        // 2. Check for line comments
        if let Some((kind, prefix)) = match_line_comment_start(trimmed_start, &syntax, options) {
            let mut block_text = String::new();
            let mut source_lines = Vec::new();

            while line_idx < lines.len() {
                let cur_line = lines[line_idx];
                let cur_trimmed = cur_line.trim_start();
                let cur_leading = cur_line.len() - cur_trimmed.len();

                if let Some((cur_kind, cur_prefix)) = match_line_comment_start(cur_trimmed, &syntax, options) {
                    if cur_kind != kind || cur_prefix != prefix {
                        break;
                    }

                    let after = &cur_trimmed[cur_prefix.len()..];
                    let (content, col_offset) = if let Some(stripped) = after.strip_prefix(' ') {
                        (stripped, cur_leading + cur_prefix.len() + 1)
                    } else {
                        (after, cur_leading + cur_prefix.len())
                    };

                    block_text.push_str(content);
                    block_text.push('\n');
                    source_lines.push((line_idx + 1, col_offset + 1));
                    line_idx += 1;
                } else {
                    break;
                }
            }

            if !block_text.is_empty() {
                blocks.push(ExtractedCommentBlock {
                    text: block_text,
                    kind,
                    source_map: CommentSourceMap {
                        file_path: file.to_path_buf(),
                        lines: source_lines,
                    },
                });
            }

            continue;
        }

        line_idx += 1;
    }

    blocks
}

fn match_block_comment_start<'a>(
    trimmed: &str,
    syntax: &'a CommentSyntax,
    options: ExtractOptions,
) -> Option<(CommentKind, &'static str, &'static str)> {
    for (start, end) in &syntax.doc_block_delimiters {
        if trimmed.starts_with(start) {
            return Some((CommentKind::OuterDoc, *start, *end));
        }
    }
    if !options.doc_only {
        for (start, end) in &syntax.regular_block_delimiters {
            if trimmed.starts_with(start) {
                return Some((CommentKind::Block, *start, *end));
            }
        }
    }
    None
}

fn match_line_comment_start(
    trimmed: &str,
    syntax: &CommentSyntax,
    options: ExtractOptions,
) -> Option<(CommentKind, &'static str)> {
    for prefix in &syntax.doc_line_prefixes {
        if trimmed.starts_with(prefix) {
            let kind = if *prefix == "//!" {
                CommentKind::InnerDoc
            } else {
                CommentKind::OuterDoc
            };
            return Some((kind, *prefix));
        }
    }
    if !options.doc_only {
        for prefix in &syntax.regular_line_prefixes {
            if trimmed.starts_with(prefix) {
                return Some((CommentKind::Line, *prefix));
            }
        }
    }
    None
}

fn strip_boundary_whitespace(s: &str, base_col: usize) -> (&str, usize) {
    let trimmed = s.trim_start();
    let leading = s.len() - trimmed.len();
    (trimmed.trim_end(), base_col + leading)
}

/// Cleans a line inside a multiline block comment (e.g. strips leading ` * ` in C-style comments).
fn clean_block_comment_line<'a>(line: &'a str, start_delim: &str) -> (String, usize) {
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();

    // If block starts with `/*` or `/**`, multiline inner lines often begin with `*` or `* `
    if (start_delim.starts_with("/*") || start_delim.starts_with("/**")) && trimmed.starts_with('*') && !trimmed.starts_with("*/") {
        let after_star = &trimmed[1..];
        if let Some(rest) = after_star.strip_prefix(' ') {
            (rest.to_string(), leading + 2)
        } else {
            (after_star.to_string(), leading + 1)
        }
    } else {
        (line.to_string(), 0)
    }
}
