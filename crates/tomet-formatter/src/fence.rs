//! Formatting fenced code block lines according to `code.fence.lines`.

use tomet_config::PrinterConfig;
use tomet_cst::{AstNode, CstCodeBlock, SyntaxKind as K};
use tomet_cst_parser::parse_cst;
use tomet_spelling::fence_line_requires_marker;
use tomet_style::FenceLines;

use crate::edit::Edit;

/// Formats the body lines of fenced code blocks according to `code.fence.lines`:
/// - `Marked`: Every line begins with ` `` ` (the canonical spelling).
/// - `Bare`: Lines are stripped of markers, except lines requiring one ([`fence_line_requires_marker`]).
/// - `Unset`: Left as written.
pub fn format_fence_lines(src: &str, config: &PrinterConfig) -> String {
    let Some(lines_style) = config.format.code_fence_lines else {
        return src.to_string();
    };

    let root = parse_cst(src);
    let mut edits: Vec<Edit> = Vec::new();

    for node in root.descendants() {
        if node.kind() != K::CODE_BLOCK {
            continue;
        }
        let Some(cst_block) = CstCodeBlock::cast(node.clone()) else {
            continue;
        };
        if cst_block.is_line_raw() {
            continue;
        }
        let Some(opening_fence) = cst_block.opening_fence() else {
            continue;
        };
        let fence_len = usize::from(opening_fence.syntax().text().len());
        let depth = cst_block.pipe_depth();

        for line in cst_block.lines() {
            let line_node = line.syntax();
            let has_marker = line.line_marker().is_some();
            let range = line_node.text_range();
            let start = usize::from(range.start());
            let end = usize::from(range.end());
            let line_text = &src[start..end];
            let newline = if line_text.ends_with("\r\n") {
                "\r\n"
            } else {
                "\n"
            };

            let Some(parts) = line.parts(depth) else {
                continue;
            };

            match lines_style {
                FenceLines::Marked => {
                    if !has_marker {
                        let new_line = if parts.body.is_empty() {
                            format!("{}``{newline}", parts.margin_prefix)
                        } else {
                            format!("{}`` {}{newline}", parts.margin_prefix, parts.body)
                        };
                        edits.push((start, end, new_line));
                    }
                }
                FenceLines::Bare => {
                    if has_marker && !fence_line_requires_marker(&parts.body, fence_len) {
                        let new_line = if parts.body.is_empty() {
                            if parts.margin_prefix.is_empty() {
                                newline.to_string()
                            } else {
                                format!("{}{newline}", parts.margin_prefix)
                            }
                        } else {
                            format!("{}{}{newline}", parts.margin_prefix, parts.body)
                        };
                        edits.push((start, end, new_line));
                    }
                }
            }
        }
    }

    if edits.is_empty() {
        return src.to_string();
    }

    let mut out = src.to_string();
    for (start, end, replacement) in edits.into_iter().rev() {
        out.replace_range(start..end, &replacement);
    }
    out
}
