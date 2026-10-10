//! Table conversion, width calculation, and Markdown table preprocessing.

use pulldown_cmark::Alignment;
use tomet_ast::{Block, Element, Inline, Placement, Sigil, Span, Text};
use tomet_tree::{ElementExt, element_new};

use super::{ImportOptions, wrap_inline_content};

pub(super) fn char_display_width(c: char) -> usize {
    if c.is_ascii() {
        1
    } else {
        // CJK fullwidth characters, emojis, etc. count as 2 columns
        2
    }
}

pub(super) fn inlines_display_width(inlines: &[Inline]) -> usize {
    let mut len = 0;
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let trimmed = t.value.trim();
                for c in trimmed.chars() {
                    len += char_display_width(c);
                }
            }
            Inline::Raw(t) => {
                for c in t.value.chars() {
                    len += char_display_width(c);
                }
            }
            // A table cell's content doesn't span source lines in practice,
            // but if it did, a fold is a joining space -- one column.
            Inline::SoftBreak(_) => len += 1,
            Inline::LineBreak(_) => {}
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    len += blocks_display_width(content);
                }
            }
        }
    }
    len
}

/// [`inlines_display_width`] over `Element.content`'s `Vec<Block>` shape.
pub(super) fn blocks_display_width(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|block| match block {
            Block::Paragraph(p) => inlines_display_width(&p.content),
            Block::Element(el) => el.content.as_deref().map(blocks_display_width).unwrap_or(0),
            Block::Section(sec) => inlines_display_width(&sec.title),
            Block::List(list) => list
                .items
                .iter()
                .map(|item| {
                    item.element
                        .content
                        .as_deref()
                        .map(blocks_display_width)
                        .unwrap_or(0)
                })
                .sum(),
        })
        .sum()
}

pub(super) fn build_table_element(
    rows: Vec<Vec<Vec<Inline>>>,
    alignments: Vec<Alignment>,
    options: &ImportOptions,
) -> Element {
    let mut content_inlines = Vec::new();
    if rows.is_empty() {
        let mut el = element_new(Sigil::named("table")).with_placement(Placement::Block);
        el.content = Some(wrap_inline_content(content_inlines));
        return el;
    }

    let mut col_count = 0;
    for row in &rows {
        col_count = col_count.max(row.len());
    }

    let default_align = options.table_align.as_deref().unwrap_or("left");

    let mode =
        options
            .table_adjust_width_mode
            .as_deref()
            .unwrap_or(if options.adjust_table_width {
                "true"
            } else {
                "false"
            });

    let max_col_width_limit = options.table_max_col_width.unwrap_or(20);

    let mut max_lens = vec![0usize; col_count];
    for row in &rows {
        for (c_idx, cell) in row.iter().enumerate() {
            let len = inlines_display_width(cell);
            max_lens[c_idx] = max_lens[c_idx].max(len);
        }
    }

    let mut target_widths: Vec<Option<usize>> = vec![None; col_count];
    let mut auto_align_active = true;

    for (c_idx, &max_len) in max_lens.iter().enumerate() {
        match mode {
            "true" | "all" => {
                target_widths[c_idx] = Some(max_len.max(1));
            }
            "auto" => {
                if auto_align_active && max_len <= max_col_width_limit {
                    target_widths[c_idx] = Some(max_len.max(1));
                } else {
                    auto_align_active = false;
                    target_widths[c_idx] = None;
                }
            }
            _ => {
                target_widths[c_idx] = None;
            }
        }
    }

    content_inlines.push(Inline::Text(Text::new("\n", Span::dummy())));

    for row in &rows {
        for (c_idx, &target) in target_widths.iter().enumerate() {
            let cell_inlines = row.get(c_idx).cloned().unwrap_or_default();
            let cell_len = inlines_display_width(&cell_inlines);

            let col_align = alignments
                .get(c_idx)
                .map(|a| match a {
                    Alignment::Right => "right",
                    Alignment::Center => "center",
                    Alignment::Left => "left",
                    Alignment::None => default_align,
                })
                .unwrap_or(default_align);

            let (left_spaces, right_spaces) = if let Some(target_w) = target {
                let target_width = target_w + 2;
                let extra = if target_width > cell_len {
                    target_width - cell_len
                } else {
                    2
                };
                match col_align {
                    "right" => {
                        let left = extra.saturating_sub(1);
                        let right = 1;
                        (left, right)
                    }
                    "center" => {
                        let left = extra / 2;
                        let right = extra - left;
                        (left, right)
                    }
                    _ => {
                        // "left"
                        let left = 1;
                        let right = extra.saturating_sub(1);
                        (left, right)
                    }
                }
            } else {
                (1, 1)
            };

            let left_str = " ".repeat(left_spaces);
            let right_str = " ".repeat(right_spaces);

            content_inlines.push(Inline::Text(Text::new(
                format!("[{left_str}"),
                Span::dummy(),
            )));
            content_inlines.extend(cell_inlines);
            content_inlines.push(Inline::Text(Text::new(
                format!("{right_str}]"),
                Span::dummy(),
            )));
        }
        content_inlines.push(Inline::Text(Text::new("\n", Span::dummy())));
    }

    let mut el = element_new(Sigil::named("table")).with_placement(Placement::Block);
    el.content = Some(wrap_inline_content(content_inlines));
    el
}

pub(super) fn preprocess_markdown_tables(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 32);
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.contains("[[") && trimmed.contains('|') {
            let mut in_wikilink = false;
            let mut chars = line.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '[' && chars.peek() == Some(&'[') {
                    out.push('[');
                    out.push(chars.next().unwrap());
                    in_wikilink = true;
                } else if in_wikilink && ch == ']' && chars.peek() == Some(&']') {
                    out.push(']');
                    out.push(chars.next().unwrap());
                    in_wikilink = false;
                } else if in_wikilink && ch == '|' && !out.ends_with('\\') {
                    out.push('\\');
                    out.push('|');
                } else {
                    out.push(ch);
                }
            }
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !src.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    out
}
