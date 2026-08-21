//! Structural Grep tab rendering: match list + refactoring diff
//! preview.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

use super::diff::generate_colored_diff;
use crate::app::{App, FocusedPane, StructuralInputTarget};

pub(super) fn render_structural_view(f: &mut Frame, app: &mut App, area: Rect) {
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
            .border_style(super::get_border_style(
                app.focused_pane == FocusedPane::List,
            ))
            .title(" Structural Search Matches "),
    );
    f.render_widget(list, chunks[0]);

    // Diff Pane
    let mut diff_lines = Vec::new();
    if let Some(m) = app.structural_matches.get(app.structural_index) {
        diff_lines.push(Line::from(Span::styled(
            format!("--- REFACTORED DIFF PREVIEW ({}) ---", m.path.display()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        diff_lines.extend(generate_colored_diff(&m.original_src, &m.modified_src));
    } else {
        diff_lines.push(Line::from("No matching elements found."));
    }

    let diff = Paragraph::new(diff_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(super::get_border_style(
                    app.focused_pane == FocusedPane::Preview,
                ))
                .title(" Refactoring Diff Preview (PageUp/Down to scroll) "),
        )
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(diff, chunks[1]);
}

pub(super) fn structural_controls_info(app: &App) -> String {
    let tag_style = if app.structural_input_target == Some(StructuralInputTarget::QueryTag) {
        "> TAG: "
    } else {
        "TAG: "
    };
    let key_style = if app.structural_input_target == Some(StructuralInputTarget::QueryKey) {
        "> KEY: "
    } else {
        "KEY: "
    };
    let val_style = if app.structural_input_target == Some(StructuralInputTarget::QueryVal) {
        "> VAL: "
    } else {
        "VAL: "
    };
    let rep_style = if app.structural_input_target == Some(StructuralInputTarget::ReplaceTo) {
        "> REPLACE: "
    } else {
        "REPLACE: "
    };
    format!(
        "{tag_style}{} | {key_style}{} | {val_style}{} | {rep_style}{}  ([t]/[k]/[v]/[r] edit, [s]earch, [c]lear, [e] Apply Refactor)",
        app.query_tag_input, app.query_key_input, app.query_val_input, app.replace_to_input
    )
}
