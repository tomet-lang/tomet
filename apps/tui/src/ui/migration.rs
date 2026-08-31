//! Migration tab rendering: file tree with selection checkboxes +
//! Markdown-to-`.tmt` conversion diff preview.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::diff::generate_colored_diff;
use crate::app::{App, FocusedPane};

pub(super) fn render_migration_view(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let list_height = chunks[0].height.saturating_sub(2) as usize;
    app.adjust_migration_scrolloff(list_height);

    let vis_indices = app.visible_migration_tree_indices();
    let visible_height = chunks[0].height.saturating_sub(2) as usize;
    let visible_items: Vec<ListItem> = vis_indices
        .iter()
        .skip(app.migration_scroll_offset)
        .take(visible_height)
        .enumerate()
        .map(|(i, &real_idx)| {
            let vis_idx = app.migration_scroll_offset + i;
            let node = &app.tree_nodes[real_idx];
            let indent = "  ".repeat(node.depth);

            let line = if node.is_dir {
                let icon = if node.expanded { "[-] " } else { "[+] " };
                let (sel_count, total_count) = app.get_dir_migration_status_for_node(real_idx);
                let checkbox = if sel_count == total_count && total_count > 0 {
                    "[x] "
                } else if sel_count > 0 {
                    "[-] "
                } else {
                    "[ ] "
                };

                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(checkbox, Style::default().fg(Color::Green)),
                    Span::styled(icon, Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("{}/", node.name),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" ({sel_count}/{total_count})"),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            } else {
                let is_selected = app
                    .migration_items
                    .iter()
                    .find(|i| i.source_path == node.path)
                    .map_or(false, |i| i.selected);

                let is_converted = app
                    .migration_items
                    .iter()
                    .find(|i| i.source_path == node.path)
                    .map_or(false, |i| i.converted);

                let checkbox = if is_selected { "[x] " } else { "[ ] " };
                let status = if is_converted { " (DONE)" } else { "" };

                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(checkbox, Style::default().fg(Color::Green)),
                    Span::raw(&node.name),
                    Span::styled(status, Style::default().fg(Color::DarkGray)),
                ])
            };

            let style = if vis_idx == app.migration_index {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let file_list = List::new(visible_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(super::get_border_style(
                app.focused_pane == FocusedPane::List,
            ))
            .title(" Migration Tree (.md -> .tmt) "),
    );
    f.render_widget(file_list, chunks[0]);

    // Preview Pane
    let mut preview_lines = Vec::new();
    let selected_real_idx = vis_indices.get(app.migration_index).copied();
    if let Some(real_idx) = selected_real_idx {
        let node = &app.tree_nodes[real_idx];
        if node.is_dir {
            let (sel_count, total_count) = app.get_dir_migration_status(&node.path);
            preview_lines.push(Line::from(Span::styled(
                format!("--- DIRECTORY SUMMARY: {} ---", node.path.display()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            preview_lines.push(Line::from(format!(
                "Selected Markdown files: {} / {}",
                sel_count, total_count
            )));
            preview_lines.push(Line::from(
                "Press [Space] to toggle selection for all files in this folder.",
            ));
            preview_lines.push(Line::from(
                "Press [Left/Right] or [Enter] to collapse/expand directory.",
            ));
        } else if let Some(item) = app
            .migration_items
            .iter_mut()
            .find(|i| i.source_path == node.path)
        {
            item.ensure_loaded_with_config(&app.printer_config);
            preview_lines.push(Line::from(Span::styled(
                format!(
                    "--- MIGRATION DIFF PREVIEW ({} -> {}) ---",
                    item.source_path.display(),
                    item.target_path.display()
                ),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            preview_lines.extend(generate_colored_diff(&item.markdown_src, &item.tomet_src));
        }
    } else {
        preview_lines.push(Line::from(
            "No Markdown files found in workspace directory.",
        ));
    }

    let preview = Paragraph::new(preview_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(super::get_border_style(
                    app.focused_pane == FocusedPane::Preview,
                ))
                .title(" Conversion Diff Preview (PageUp/Down to scroll) "),
        )
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(preview, chunks[1]);
}

pub(super) fn migration_controls_info(_app: &App) -> String {
    "[Space/Enter] Toggle File/Folder | [Left/Right] Expand/Collapse | [a] Toggle All | [e] Convert selected to .tmt"
        .to_string()
}
