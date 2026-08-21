//! Batch Meta tab rendering: `.tm` file list with metadata field
//! counts + metadata detail/diff preview.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::diff::generate_colored_diff;
use crate::app::{App, FocusedPane, MetaEditTarget};

pub(super) fn render_batch_meta_view(f: &mut Frame, app: &mut App, area: Rect) {
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
            .border_style(super::get_border_style(
                app.focused_pane == FocusedPane::List,
            ))
            .title(" TypedMark Files (.tm) "),
    );
    f.render_widget(list, chunks[0]);

    // Metadata Detail & Diff Pane
    let mut lines = Vec::new();
    if let Some(entry) = app.meta_entries.get_mut(app.meta_index) {
        entry.ensure_loaded();
        lines.push(Line::from(Span::styled(
            format!("--- METADATA FIELDS ({}) ---", entry.path.display()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
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
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )));
        lines.extend(generate_colored_diff(
            &entry.original_src,
            &entry.modified_src,
        ));
    } else {
        lines.push(Line::from("No .tm files found in directory."));
    }

    let detail = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(super::get_border_style(
                    app.focused_pane == FocusedPane::Preview,
                ))
                .title(" Metadata & Source Diff Preview (PageUp/Down to scroll) "),
        )
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(detail, chunks[1]);
}

pub(super) fn batch_meta_controls_info(app: &App) -> String {
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
