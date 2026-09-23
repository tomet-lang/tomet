//! Provides folding ranges for sections, blocks, and comments in the editor.

use lsp_types::{FoldingRange, FoldingRangeKind};
use tomet_ast::{Block, ElementValue};

/// Computes folding ranges for the given document text.
pub fn folding_ranges_for(text: &str) -> Vec<FoldingRange> {
    let Ok(doc) = tomet_parser::parse_document(text) else {
        return Vec::new();
    };

    let mut ranges = Vec::new();
    for block in &doc.blocks {
        collect_block_folding_ranges(block, &mut ranges);
    }

    collect_comment_folding_ranges(text, &mut ranges);

    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges
}

fn collect_block_folding_ranges(block: &Block, ranges: &mut Vec<FoldingRange>) {
    match block {
        Block::Section(sec) => {
            let start_line = sec.span.start.line.saturating_sub(1) as u32;
            let end_line = if let Some(last) = sec.blocks.last() {
                block_end_line(last)
            } else {
                sec.span.end.line.saturating_sub(1) as u32
            };

            if end_line > start_line {
                ranges.push(FoldingRange {
                    start_line,
                    start_character: None,
                    end_line,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: None,
                });
            }

            for child in &sec.blocks {
                collect_block_folding_ranges(child, ranges);
            }
        }
        Block::Element(el) => {
            let start_line = el.span.start.line.saturating_sub(1) as u32;
            let end_line = span_end_line(&el.span);

            if end_line > start_line {
                ranges.push(FoldingRange {
                    start_line,
                    start_character: None,
                    end_line,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: None,
                });
            }

            if let Some(val) = &el.value {
                collect_element_value_folding_ranges(val, ranges);
            }
        }
        Block::Paragraph(_) => {}
    }
}

fn collect_element_value_folding_ranges(val: &ElementValue, ranges: &mut Vec<FoldingRange>) {
    if let ElementValue::Group(entries) = val {
        for entry in entries {
            if let tomet_ast::Entry::Element(child) = entry {
                let start_line = child.span.start.line.saturating_sub(1) as u32;
                let end_line = child.span.end.line.saturating_sub(1) as u32;
                if end_line > start_line {
                    ranges.push(FoldingRange {
                        start_line,
                        start_character: None,
                        end_line,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: None,
                    });
                }
                if let Some(child_val) = &child.value {
                    collect_element_value_folding_ranges(child_val, ranges);
                }
            }
        }
    }
}

fn block_end_line(block: &Block) -> u32 {
    match block {
        Block::Section(sec) => {
            if let Some(last) = sec.blocks.last() {
                block_end_line(last)
            } else {
                span_end_line(&sec.span)
            }
        }
        Block::Element(el) => span_end_line(&el.span),
        Block::Paragraph(p) => span_end_line(&p.span),
    }
}

fn span_end_line(span: &tomet_ast::Span) -> u32 {
    let raw_end = span.end.line.saturating_sub(1) as u32;
    if span.end.column <= 1 && span.end.line > span.start.line {
        raw_end.saturating_sub(1)
    } else {
        raw_end
    }
}

fn collect_comment_folding_ranges(text: &str, ranges: &mut Vec<FoldingRange>) {
    let mut in_block_comment = false;
    let mut block_comment_start = 0;

    let mut line_comment_start: Option<u32> = None;
    let mut last_line_comment = 0;

    for (line_idx, line) in text.lines().enumerate() {
        let line_num = line_idx as u32;
        let trimmed = line.trim();

        if !in_block_comment {
            if trimmed.starts_with("/*") {
                in_block_comment = true;
                block_comment_start = line_num;
                if trimmed.contains("*/") && !trimmed.ends_with("/*") {
                    in_block_comment = false;
                }
            } else if trimmed.starts_with("//") {
                match line_comment_start {
                    None => {
                        line_comment_start = Some(line_num);
                        last_line_comment = line_num;
                    }
                    Some(_) => last_line_comment = line_num,
                }
            } else {
                if let Some(start) = line_comment_start {
                    if last_line_comment > start {
                        ranges.push(FoldingRange {
                            start_line: start,
                            start_character: None,
                            end_line: last_line_comment,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Comment),
                            collapsed_text: None,
                        });
                    }
                    line_comment_start = None;
                }
            }
        } else if trimmed.contains("*/") {
            in_block_comment = false;
            if line_num > block_comment_start {
                ranges.push(FoldingRange {
                    start_line: block_comment_start,
                    start_character: None,
                    end_line: line_num,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Comment),
                    collapsed_text: None,
                });
            }
        }
    }

    if let Some(start) = line_comment_start {
        if last_line_comment > start {
            ranges.push(FoldingRange {
                start_line: start,
                start_character: None,
                end_line: last_line_comment,
                end_character: None,
                kind: Some(FoldingRangeKind::Comment),
                collapsed_text: None,
            });
        }
    }
}
