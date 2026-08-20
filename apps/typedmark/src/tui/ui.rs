//! Ratatui UI Layout and widget rendering for TypedMark Workbench TUI.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
};

use std::path::Path;
use tree_sitter::StreamingIterator;

use super::app::{
    ActiveTab, App, FocusedPane, MetaEditTarget, PendingConfirm, StructuralInputTarget,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header & Tabs
            Constraint::Min(10),   // Main split content
            Constraint::Length(3), // Input / Controls panel
            Constraint::Length(1), // Footer status bar
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);

    match app.active_tab {
        ActiveTab::Explorer => render_explorer_view(f, app, chunks[1]),
        ActiveTab::Migration => render_migration_view(f, app, chunks[1]),
        ActiveTab::BatchMeta => render_batch_meta_view(f, app, chunks[1]),
        ActiveTab::StructuralGrep => render_structural_view(f, app, chunks[1]),
    }

    render_controls(f, app, chunks[2]);
    render_footer(f, app, chunks[3]);

    if let Some(confirm) = &app.pending_confirm {
        render_confirm_modal(f, confirm, f.area());
    }
}

fn get_border_style(is_focused: bool) -> Style {
    if is_focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec![
        " 1. Workspace Explorer ",
        " 2. Migration (MD -> TM) ",
        " 3. Batch Meta Editor ",
        " 4. Structural Grep ",
    ];

    let config_title = if let Some(path) = &app.printer_config_path {
        format!(" TypedMark Workbench TUI (Config: {}) ", path.display())
    } else {
        " TypedMark Workbench TUI (Config: default) ".to_string()
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(config_title),
        )
        .select(app.active_tab as usize)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

fn render_explorer_view(f: &mut Frame, app: &mut App, area: Rect) {
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
                let icon = if node.expanded { "[-] 📁 " } else { "[+] 📁 " };
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
            } else if let Some(item) = &node.migration_item {
                let status = if item.converted { " (DONE)" } else { "" };
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
            .border_style(get_border_style(app.focused_pane == FocusedPane::List))
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
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                preview_lines.push(Line::from("Directory contents visible in workspace tree."));
                preview_lines.push(Line::from("Press [Enter] / [Space] to expand or collapse."));
            } else {
                preview_lines.push(Line::from(Span::styled(
                    format!("--- FILE PREVIEW ({}) ---", node.path.display()),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
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
                .border_style(get_border_style(app.focused_pane == FocusedPane::Preview))
                .title(title),
        )
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(preview, chunks[1]);
}

fn render_migration_view(f: &mut Frame, app: &mut App, area: Rect) {
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
            .border_style(get_border_style(app.focused_pane == FocusedPane::List))
            .title(" Migration Tree (.md -> .tm) "),
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
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
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
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
            preview_lines.extend(generate_colored_diff(&item.markdown_src, &item.typedmark_src));
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
                .border_style(get_border_style(app.focused_pane == FocusedPane::Preview))
                .title(" Conversion Diff Preview (PageUp/Down to scroll) "),
        )
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(preview, chunks[1]);
}

fn render_batch_meta_view(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let list_height = chunks[0].height.saturating_sub(2) as usize;
    app.adjust_meta_scrolloff(list_height);

    let items: Vec<ListItem> = app
        .meta_entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let checkbox = if entry.selected { "[x] " } else { "[ ] " };
            let filename = entry
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");

            let meta_count = entry.metadata.len();
            let line = Line::from(vec![
                Span::styled(checkbox, Style::default().fg(Color::Green)),
                Span::raw(filename),
                Span::styled(
                    format!(" ({meta_count} meta fields)"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            let style = if idx == app.meta_index {
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

    let visible_height = chunks[0].height.saturating_sub(2) as usize;
    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(app.meta_scroll_offset)
        .take(visible_height)
        .collect();

    let list = List::new(visible_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(get_border_style(app.focused_pane == FocusedPane::List))
            .title(" TypedMark Files (.tm) "),
    );
    f.render_widget(list, chunks[0]);

    // Metadata Detail & Diff Pane
    let mut lines = Vec::new();
    if let Some(entry) = app.meta_entries.get_mut(app.meta_index) {
        entry.ensure_loaded();
        lines.push(Line::from(Span::styled(
            format!("--- METADATA FIELDS ({}) ---", entry.path.display()),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        if entry.metadata.is_empty() {
            lines.push(Line::from(Span::styled(
                "(No @meta / @config fields found)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (k, v) in &entry.metadata {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{k}: "),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(v.to_string()),
                ]));
            }
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "--- MODIFIED DIFF PREVIEW ---",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )));
        lines.extend(generate_colored_diff(&entry.original_src, &entry.modified_src));
    } else {
        lines.push(Line::from("No .tm files found in directory."));
    }

    let detail = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(get_border_style(app.focused_pane == FocusedPane::Preview))
                .title(" Metadata & Source Diff Preview (PageUp/Down to scroll) "),
        )
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(detail, chunks[1]);
}

fn render_structural_view(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let list_height = chunks[0].height.saturating_sub(2) as usize;
    app.adjust_structural_scrolloff(list_height);

    let items: Vec<ListItem> = app
        .structural_matches
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let checkbox = if m.selected { "[x] " } else { "[ ] " };
            let filename = m
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");

            let line = Line::from(vec![
                Span::styled(checkbox, Style::default().fg(Color::Green)),
                Span::raw(filename),
                Span::styled(
                    format!(" ({} match)", m.match_count),
                    Style::default().fg(Color::Yellow),
                ),
            ]);

            let style = if idx == app.structural_index {
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

    let visible_height = chunks[0].height.saturating_sub(2) as usize;
    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(app.structural_scroll_offset)
        .take(visible_height)
        .collect();

    let list = List::new(visible_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(get_border_style(app.focused_pane == FocusedPane::List))
            .title(" Structural Search Matches "),
    );
    f.render_widget(list, chunks[0]);

    // Diff Pane
    let mut diff_lines = Vec::new();
    if let Some(m) = app.structural_matches.get(app.structural_index) {
        diff_lines.push(Line::from(Span::styled(
            format!("--- REFACTORED DIFF PREVIEW ({}) ---", m.path.display()),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        diff_lines.extend(generate_colored_diff(&m.original_src, &m.modified_src));
    } else {
        diff_lines.push(Line::from("No matching elements found."));
    }

    let diff = Paragraph::new(diff_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(get_border_style(app.focused_pane == FocusedPane::Preview))
                .title(" Refactoring Diff Preview (PageUp/Down to scroll) "),
        )
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(diff, chunks[1]);
}

fn generate_colored_diff(original: &str, modified: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if original == modified {
        for line in original.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(Color::DarkGray),
            )));
        }
        return lines;
    }

    let ops = compute_line_diff(original, modified);
    for op in ops {
        match op {
            DiffOp::Keep(line) => {
                lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            DiffOp::Delete(line) => {
                lines.push(Line::from(Span::styled(
                    format!("- {line}"),
                    Style::default().fg(Color::Red),
                )));
            }
            DiffOp::Insert(line) => {
                lines.push(Line::from(Span::styled(
                    format!("+ {line}"),
                    Style::default().fg(Color::Green),
                )));
            }
        }
    }
    lines
}

#[derive(Debug, PartialEq, Eq)]
enum DiffOp<'a> {
    Keep(&'a str),
    Delete(&'a str),
    Insert(&'a str),
}

fn compute_line_diff<'a>(old_text: &'a str, new_text: &'a str) -> Vec<DiffOp<'a>> {
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();
    let n = old_lines.len();
    let m = new_lines.len();

    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            if old_lines[i] == new_lines[j] {
                table[i][j] = table[i + 1][j + 1] + 1;
            } else {
                table[i][j] = table[i + 1][j].max(table[i][j + 1]);
            }
        }
    }

    let mut ops = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if old_lines[i] == new_lines[j] {
            ops.push(DiffOp::Keep(old_lines[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            ops.push(DiffOp::Delete(old_lines[i]));
            i += 1;
        } else {
            ops.push(DiffOp::Insert(new_lines[j]));
            j += 1;
        }
    }
    while i < n {
        ops.push(DiffOp::Delete(old_lines[i]));
        i += 1;
    }
    while j < m {
        ops.push(DiffOp::Insert(new_lines[j]));
        j += 1;
    }

    ops
}

fn render_controls(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.active_tab {
        ActiveTab::Explorer => {
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
        ActiveTab::Migration => {
            "[Space/Enter] Toggle File/Folder | [Left/Right] Expand/Collapse | [a] Toggle All | [e] Convert selected to .tm"
                .to_string()
        }
        ActiveTab::BatchMeta => {
            let key_style = if app.meta_edit_target == Some(MetaEditTarget::Key) {
                "> KEY: "
            } else {
                "KEY: "
            };
            let val_style = if app.meta_edit_target == Some(MetaEditTarget::Value) {
                "> VAL: "
            } else {
                "VAL: "
            };
            format!(
                "{key_style}{} | {val_style}{}  ([k]ey/[v]al edit mode, [e] Apply @meta update)",
                app.meta_key_input, app.meta_val_input
            )
        }
        ActiveTab::StructuralGrep => {
            let tag_style = if app.structural_input_target == Some(StructuralInputTarget::QueryTag)
            {
                "> TAG: "
            } else {
                "TAG: "
            };
            let key_style = if app.structural_input_target == Some(StructuralInputTarget::QueryKey)
            {
                "> KEY: "
            } else {
                "KEY: "
            };
            let val_style = if app.structural_input_target == Some(StructuralInputTarget::QueryVal)
            {
                "> VAL: "
            } else {
                "VAL: "
            };
            let rep_style = if app.structural_input_target == Some(StructuralInputTarget::ReplaceTo)
            {
                "> REPLACE: "
            } else {
                "REPLACE: "
            };
            format!(
                "{tag_style}{} | {key_style}{} | {val_style}{} | {rep_style}{}  ([t]/[k]/[v]/[r] edit, [s]earch, [c]lear, [e] Apply Refactor)",
                app.query_tag_input, app.query_key_input, app.query_val_input, app.replace_to_input
            )
        }
    };

    let p = Paragraph::new(info).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Quick Actions & Controls "),
    );
    f.render_widget(p, area);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let p = Paragraph::new(Span::styled(
        &app.status_message,
        Style::default().fg(Color::Yellow),
    ));
    f.render_widget(p, area);
}

fn render_confirm_modal(f: &mut Frame, confirm: &PendingConfirm, area: Rect) {
    let block = Block::default()
        .title(" Action Confirmation ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let width = 60.min(area.width.saturating_sub(4));
    let height = 7.min(area.height.saturating_sub(2));

    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    f.render_widget(Clear, popup_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", confirm.message),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [y] ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Confirm & Execute    "),
            Span::styled(
                "[n / Esc] ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cancel"),
        ]),
    ];

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup_area);
}

fn highlight_source_file(path: &Path, src: &str) -> Vec<Line<'static>> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_tm = ext.eq_ignore_ascii_case("tm") || ext.eq_ignore_ascii_case("tmt");

    if is_tm {
        highlight_typedmark_tree_sitter(src)
    } else {
        fallback_typedmark_highlight(src)
    }
}

fn highlight_typedmark_tree_sitter(src: &str) -> Vec<Line<'static>> {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_typedmark::LANGUAGE.into();

    if parser.set_language(&language).is_err() {
        return fallback_typedmark_highlight(src);
    }
    let tree = match parser.parse(src, None) {
        Some(t) => t,
        None => return fallback_typedmark_highlight(src),
    };

    let query = match tree_sitter::Query::new(&language, tree_sitter_typedmark::HIGHLIGHTS_QUERY) {
        Ok(q) => q,
        Err(_) => return fallback_typedmark_highlight(src),
    };

    let src_bytes = src.as_bytes();
    let src_len = src_bytes.len();
    let mut byte_styles = vec![Style::default(); src_len];

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut captures = cursor.captures(&query, tree.root_node(), src_bytes);
    while let Some((m, cap_idx)) = captures.next() {
        let capture = m.captures[*cap_idx];
        let cap_name = query.capture_names()[capture.index as usize];
        let style = match cap_name {
            "comment" => Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            "tag" => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            "title" | "markup.heading" => {
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
            }
            "string" | "string.special" => Style::default().fg(Color::Green),
            "punctuation.special" | "punctuation.bracket" | "punctuation.delimiter" => {
                Style::default().fg(Color::Yellow)
            }
            "property" => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            "constant" | "markup.list.checked" => Style::default().fg(Color::Green),
            "markup.list.unnumbered" | "markup.list.numbered" => {
                Style::default().fg(Color::Blue)
            }
            "text.literal" => Style::default().fg(Color::Gray),
            _ => Style::default(),
        };

        let range = capture.node.start_byte()..capture.node.end_byte();
        for idx in range {
            if idx < src_len {
                byte_styles[idx] = style;
            }
        }
    }

    let mut lines = Vec::new();
    let mut current_pos = 0;

    for line_str in src.lines() {
        let line_len = line_str.len();
        let line_end = current_pos + line_len;

        if line_len == 0 {
            lines.push(Line::from(""));
        } else {
            let mut spans = Vec::new();
            let mut chunk_start = current_pos;
            let mut current_style = byte_styles[current_pos];

            for b_idx in current_pos..line_end {
                let s = byte_styles[b_idx];
                if s != current_style {
                    let chunk_str = &src[chunk_start..b_idx];
                    spans.push(Span::styled(chunk_str.to_string(), current_style));
                    chunk_start = b_idx;
                    current_style = s;
                }
            }
            if chunk_start < line_end {
                let chunk_str = &src[chunk_start..line_end];
                spans.push(Span::styled(chunk_str.to_string(), current_style));
            }
            lines.push(Line::from(spans));
        }

        current_pos = line_end;
        if current_pos < src_len && src_bytes[current_pos] == b'\r' {
            current_pos += 1;
        }
        if current_pos < src_len && src_bytes[current_pos] == b'\n' {
            current_pos += 1;
        }
    }

    lines
}

fn fallback_typedmark_highlight(src: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for line_str in src.lines() {
        let trimmed = line_str.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default().fg(Color::Gray),
            )));
            continue;
        }

        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
            continue;
        }

        if trimmed.starts_with('#') {
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        if trimmed.starts_with('@') || trimmed.starts_with('<') {
            lines.push(highlight_typedmark_line(line_str));
            continue;
        }

        if trimmed.starts_with('>') {
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::ITALIC),
            )));
            continue;
        }

        if trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || (trimmed.len() >= 3
                && trimmed.as_bytes()[0].is_ascii_digit()
                && trimmed.as_bytes()[1] == b'.'
                && trimmed.as_bytes()[2] == b' ')
        {
            let prefix_len = line_str.find(|c: char| c != ' ').unwrap_or(0);
            let indent = &line_str[..prefix_len];
            let rest = &line_str[prefix_len..];

            let (marker, content) = rest.split_at(2);
            lines.push(Line::from(vec![
                Span::raw(indent.to_string()),
                Span::styled(
                    marker.to_string(),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(content.to_string()),
            ]));
            continue;
        }

        if let Some((k, v)) = line_str.split_once(':') {
            if !k.contains(' ')
                && !k.starts_with("http")
                && (v.trim_start().starts_with('"')
                    || v.trim_start().starts_with('{')
                    || v.trim_start().starts_with('[')
                    || !v.trim().is_empty())
            {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{k}:"),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(v.to_string()),
                ]));
                continue;
            }
        }

        lines.push(Line::from(Span::raw(line_str.to_string())));
    }

    lines
}

fn highlight_typedmark_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if ch == '@' || ch == '<' || (ch == '$' && i + 1 < len && chars[i + 1] == '{') {
            let start = i;
            if ch == '@' || ch == '<' {
                i += 1;
                while i < len
                    && (chars[i].is_alphanumeric()
                        || chars[i] == '_'
                        || chars[i] == '-'
                        || chars[i] == '>')
                {
                    let end_sigil = chars[i] == '>';
                    i += 1;
                    if end_sigil {
                        break;
                    }
                }
            } else {
                i += 2;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(
                text,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        if ch == '(' {
            let start = i;
            i += 1;
            let mut depth = 1;
            while i < len && depth > 0 {
                if chars[i] == '(' {
                    depth += 1;
                } else if chars[i] == ')' {
                    depth -= 1;
                }
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(text, Style::default().fg(Color::Green)));
            continue;
        }

        if ch == '{' {
            let start = i;
            i += 1;
            let mut depth = 1;
            while i < len && depth > 0 {
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    depth -= 1;
                }
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(text, Style::default().fg(Color::LightGreen)));
            continue;
        }

        if ch == '"' || ch == '\'' {
            let quote = ch;
            let start = i;
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(text, Style::default().fg(Color::Green)));
            continue;
        }

        let start = i;
        while i < len
            && chars[i] != '@'
            && chars[i] != '<'
            && chars[i] != '('
            && chars[i] != '{'
            && chars[i] != '"'
            && chars[i] != '\''
            && !(chars[i] == '$' && i + 1 < len && chars[i + 1] == '{')
        {
            i += 1;
        }
        let text: String = chars[start..i].iter().collect();
        spans.push(Span::raw(text));
    }

    Line::from(spans)
}
