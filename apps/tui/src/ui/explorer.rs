//! Explorer tab rendering: workspace file tree + syntax-highlighted
//! file preview / inline editor pane.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::highlight::highlight_source_file;
use crate::app::{App, FocusedPane};

pub(super) fn render_explorer_view(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let list_height = chunks[0].height.saturating_sub(2) as usize;
    let preview_height = chunks[1].height.saturating_sub(2) as usize;

    app.adjust_tree_scrolloff(list_height);
    app.adjust_preview_scrolloff(preview_height);

    let vis_indices = app.visible_tree_indices();
    let visible_height = chunks[0].height.saturating_sub(2) as usize;
    let visible_items: Vec<ListItem> = vis_indices
        .iter()
        .skip(app.tree_scroll_offset)
        .take(visible_height)
        .enumerate()
        .map(|(i, &real_idx)| {
            let vis_idx = app.tree_scroll_offset + i;
            let node = &app.tree_nodes[real_idx];
            let indent = "  ".repeat(node.depth);

            let line_spans = if node.is_dir {
                let icon = if node.expanded {
                    "[-] 📁 "
                } else {
                    "[+] 📁 "
                };
                vec![
                    Span::raw(indent),
                    Span::styled(icon, Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("{}/", node.name),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]
            } else if node.migration_item.is_some() {
                let converted = app
                    .migration_items
                    .iter()
                    .find(|i| i.source_path == node.path)
                    .map_or(false, |i| i.converted);
                let status = if converted { " (DONE)" } else { "" };
                vec![
                    Span::raw(indent),
                    Span::styled("📝 ", Style::default()),
                    Span::raw(&node.name),
                    Span::styled(status, Style::default().fg(Color::DarkGray)),
                ]
            } else {
                vec![
                    Span::raw(indent),
                    Span::styled("📄 ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&node.name, Style::default().fg(Color::DarkGray)),
                ]
            };

            let style = if vis_idx == app.tree_index {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(line_spans)).style(style)
        })
        .collect();

    let tree_title = if !app.tree_filter_input.is_empty() {
        format!(
            " Workspace Explorer (Filter: \"{}\" - {}/{} items) ",
            app.tree_filter_input,
            vis_indices.len(),
            app.tree_nodes.len()
        )
    } else {
        " Workspace File Explorer (Press [/] to Filter) ".to_string()
    };

    let file_list = List::new(visible_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(super::get_border_style(
                app.focused_pane == FocusedPane::List,
            ))
            .title(tree_title),
    );
    f.render_widget(file_list, chunks[0]);

    // Preview Pane
    let mut preview_lines = Vec::new();

    let title = if let Some(editor) = &app.inline_editor {
        format!(
            " EDITING: {} {} (Ctrl+S: Save, Esc: Exit) ",
            editor.file_path.display(),
            if editor.is_dirty { "[*MODIFIED*]" } else { "" }
        )
    } else {
        " File Content Preview (Press [e] to Edit, [E] Open External $EDITOR) ".to_string()
    };

    if let Some(editor) = &app.inline_editor {
        let full_src = editor.lines.join("\n");
        let highlighted = highlight_source_file(&editor.file_path, &full_src);

        for (row_idx, line) in highlighted.into_iter().enumerate() {
            if row_idx == editor.cursor_row {
                let mut cursor_line_spans = Vec::new();
                let mut char_count = 0;
                let mut cursor_rendered = false;

                for span in line.spans {
                    let text = span.content.to_string();
                    let span_len = text.chars().count();

                    if !cursor_rendered
                        && editor.cursor_col >= char_count
                        && editor.cursor_col < char_count + span_len
                    {
                        let offset = editor.cursor_col - char_count;
                        let byte_offset = text
                            .char_indices()
                            .nth(offset)
                            .map(|(i, _)| i)
                            .unwrap_or_else(|| text.len());
                        let (before, rest) = text.split_at(byte_offset);
                        if !before.is_empty() {
                            cursor_line_spans.push(Span::styled(before.to_string(), span.style));
                        }

                        let mut rest_chars = rest.chars();
                        if let Some(c_char) = rest_chars.next() {
                            cursor_line_spans.push(Span::styled(
                                c_char.to_string(),
                                Style::default()
                                    .bg(Color::Yellow)
                                    .fg(Color::Black)
                                    .add_modifier(Modifier::BOLD),
                            ));
                            let remaining: String = rest_chars.collect();
                            if !remaining.is_empty() {
                                cursor_line_spans.push(Span::styled(remaining, span.style));
                            }
                        } else {
                            cursor_line_spans.push(Span::styled(
                                " ",
                                Style::default()
                                    .bg(Color::Yellow)
                                    .fg(Color::Black)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        }
                        cursor_rendered = true;
                    } else {
                        cursor_line_spans.push(span);
                    }
                    char_count += span_len;
                }

                if !cursor_rendered {
                    cursor_line_spans.push(Span::styled(
                        " ",
                        Style::default()
                            .bg(Color::Yellow)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                preview_lines.push(Line::from(cursor_line_spans));
            } else {
                preview_lines.push(line);
            }
        }
    } else if let Some(&real_idx) = vis_indices.get(app.tree_index) {
        if let Some(node) = app.tree_nodes.get(real_idx) {
            if node.is_dir {
                preview_lines.push(Line::from(Span::styled(
                    format!("--- DIRECTORY ({}) ---", node.path.display()),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )));
                preview_lines.push(Line::from("Directory contents visible in workspace tree."));
                preview_lines.push(Line::from("Press [Enter] / [Space] to expand or collapse."));
            } else {
                preview_lines.push(Line::from(Span::styled(
                    format!("--- FILE PREVIEW ({}) ---", node.path.display()),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )));
                if let Ok(src) = std::fs::read_to_string(&node.path) {
                    preview_lines.extend(highlight_source_file(&node.path, &src));
                } else {
                    preview_lines.push(Line::from("(Binary or unreadable file)"));
                }
            }
        }
    } else {
        preview_lines.push(Line::from("No files found in workspace directory."));
    }

    let preview = Paragraph::new(preview_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(super::get_border_style(
                    app.focused_pane == FocusedPane::Preview,
                ))
                .title(title),
        )
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(preview, chunks[1]);
}

pub(super) fn explorer_controls_info(app: &App) -> String {
    if app.is_filtering_tree {
        format!(
            "SEARCH FILTER: [ {} | ] (Type to search, [Enter] Apply, [Esc] Clear filter)",
            app.tree_filter_input
        )
    } else if app.inline_editor.is_some() {
        "[Type] Edit Text | [Arrows] Move Cursor | [Enter] Newline | [Backspace/Del] Delete | [Ctrl+S] Save | [Esc] Exit Edit"
            .to_string()
    } else if !app.tree_filter_input.is_empty() {
        format!(
            "Filtered: \"{}\" ({}/{} items) | [/] Edit Filter | [Esc] Clear Filter | [e] Edit | [E] $EDITOR",
            app.tree_filter_input,
            app.visible_tree_indices().len(),
            app.tree_nodes.len()
        )
    } else {
        "[Enter/Space] Expand/Collapse | [/] Filter | [e] Edit in TUI | [E] Open External $EDITOR | [q] Quit"
            .to_string()
    }
}
