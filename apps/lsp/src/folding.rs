//! Provides folding ranges for sections, blocks, and comments in the editor.

use lsp_types::{FoldingRange, FoldingRangeKind};
use tomet_ast::{Block, Group, List};
use tomet_position::LineIndex;
use tomet_span::Span;

/// Computes folding ranges for the given document text.
pub fn folding_ranges_for(text: &str) -> Vec<FoldingRange> {
    let Ok(doc) = tomet_parser::parse_document(text) else {
        return Vec::new();
    };

    let mut ranges = Vec::new();
    let index = &LineIndex::new(text);
    for block in &doc.blocks {
        collect_block_folding_ranges(index, block, &mut ranges);
    }

    collect_comment_folding_ranges(text, &mut ranges);

    ranges.sort_by_key(|r| (r.start_line, r.end_line));
    ranges
}

fn collect_block_folding_ranges(
    index: &LineIndex<'_>,
    block: &Block,
    ranges: &mut Vec<FoldingRange>,
) {
    match block {
        Block::Section(sec) => {
            let start_line = line0(index, sec.span.start);
            let end_line = if let Some(last) = sec.blocks.last() {
                block_end_line(index, last)
            } else {
                line0(index, sec.span.end)
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
                collect_block_folding_ranges(index, child, ranges);
            }
        }
        Block::Element(el) => {
            let start_line = line0(index, el.span.start);
            let end_line = span_end_line(index, &el.span);

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

            if let Some(val) = &el.data.value {
                collect_element_value_folding_ranges(index, val, ranges);
            }
            // What the element holds folds on its own too: a section or a
            // list inside `@x[ ... ]` is a place to fold, not only the `@x`.
            for child in el.content.iter().flatten() {
                collect_block_folding_ranges(index, child, ranges);
            }
        }
        Block::Paragraph(_) => {}
        // A declaration is a line or a few; what it holds folds on its own.
        Block::Declaration(declaration) => {
            for child in declaration.content.iter().flatten() {
                collect_block_folding_ranges(index, child, ranges);
            }
        }
        Block::List(list) => collect_list_folding_ranges(index, list, ranges),
        Block::Interp(interp) => {
            let start_line = line0(index, interp.span.start);
            let end_line = span_end_line(index, &interp.span);

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
        }
        Block::Hole(_) => {}
    }
}

fn collect_list_folding_ranges(index: &LineIndex<'_>, list: &List, ranges: &mut Vec<FoldingRange>) {
    let start_line = line0(index, list.span.start);
    let end_line = span_end_line(index, &list.span);

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

    for item in &list.items {
        if let Some(val) = &item.data.value {
            collect_element_value_folding_ranges(index, val, ranges);
        }
        for child in &item.content {
            collect_block_folding_ranges(index, child, ranges);
        }
        if let Some(sub) = &item.sublist {
            collect_list_folding_ranges(index, sub, ranges);
        }
    }
}

fn collect_element_value_folding_ranges(
    index: &LineIndex<'_>,
    val: &Group,
    ranges: &mut Vec<FoldingRange>,
) {
    let Group(entries) = val;
    for entry in entries {
        if let tomet_ast::Entry::Element(child) = entry {
            let start_line = line0(index, child.span.start);
            let end_line = line0(index, child.span.end);
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
            if let Some(child_val) = &child.data.value {
                collect_element_value_folding_ranges(index, child_val, ranges);
            }
        }
    }
}

fn block_end_line(index: &LineIndex<'_>, block: &Block) -> u32 {
    match block {
        Block::Section(sec) => {
            if let Some(last) = sec.blocks.last() {
                block_end_line(index, last)
            } else {
                span_end_line(index, &sec.span)
            }
        }
        Block::Element(el) => span_end_line(index, &el.span),
        Block::Paragraph(p) => span_end_line(index, &p.span),
        Block::List(list) => span_end_line(index, &list.span),
        Block::Interp(interp) => span_end_line(index, &interp.span),
        Block::Declaration(declaration) => span_end_line(index, &declaration.span),
        Block::Hole(hole) => span_end_line(index, &hole.span),
    }
}

fn span_end_line(index: &LineIndex<'_>, span: &Span) -> u32 {
    let end = index.line_col(span.end);
    let raw_end = end.line.saturating_sub(1) as u32;
    if end.column <= 1 && end.line > index.line_col(span.start).line {
        raw_end.saturating_sub(1)
    } else {
        raw_end
    }
}

/// The 0-based line of byte `offset`, as LSP counts lines.
fn line0(index: &LineIndex<'_>, offset: usize) -> u32 {
    index.line_col(offset).line.saturating_sub(1) as u32
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

    if let Some(start) = line_comment_start
        && last_line_comment > start
    {
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
