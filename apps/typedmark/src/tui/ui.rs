//! Ratatui UI Layout and widget rendering for TypedMark Workbench TUI.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
};

use super::app::{ActiveTab, App, MetaEditTarget, StructuralInputTarget};

pub fn draw(f: &mut Frame, app: &App) {
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
        ActiveTab::Migration => render_migration_view(f, app, chunks[1]),
        ActiveTab::BatchMeta => render_batch_meta_view(f, app, chunks[1]),
        ActiveTab::StructuralGrep => render_structural_view(f, app, chunks[1]),
    }

    render_controls(f, app, chunks[2]);
    render_footer(f, app, chunks[3]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec![
        " 1. Migration (MD -> TM) ",
        " 2. Batch Meta Editor ",
        " 3. Structural Grep & Replace ",
    ];

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" TypedMark Workbench TUI "),
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

fn render_migration_view(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let items: Vec<ListItem> = app
        .migration_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let checkbox = if item.selected { "[x] " } else { "[ ] " };
            let status = if item.converted { " (DONE)" } else { "" };
            let filename = item
                .source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");

            let line = Line::from(vec![
                Span::styled(checkbox, Style::default().fg(Color::Green)),
                Span::raw(filename),
                Span::styled(status, Style::default().fg(Color::DarkGray)),
            ]);

            let style = if idx == app.migration_index {
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

    let file_list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Markdown Files "),
    );
    f.render_widget(file_list, chunks[0]);

    // Preview Pane
    let preview_text = if let Some(item) = app.migration_items.get(app.migration_index) {
        format!(
            "--- SOURCE MARKDOWN ({}) ---\n{}\n\n--- CONVERTED TYPEDMARK ({}) ---\n{}",
            item.source_path.display(),
            item.markdown_src,
            item.target_path.display(),
            item.typedmark_src
        )
    } else {
        "No Markdown files found in workspace directory.".to_string()
    };

    let preview = Paragraph::new(preview_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Conversion Preview "),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(preview, chunks[1]);
}

fn render_batch_meta_view(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

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

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" TypedMark Files (.tm) "),
    );
    f.render_widget(list, chunks[0]);

    // Metadata Detail & Diff Pane
    let content = if let Some(entry) = app.meta_entries.get(app.meta_index) {
        let mut text = format!("--- METADATA FIELDS ({}) ---\n", entry.path.display());
        if entry.metadata.is_empty() {
            text.push_str("(No @meta / @config fields found)\n");
        } else {
            for (k, v) in &entry.metadata {
                text.push_str(&format!("{k}: {v}\n"));
            }
        }
        text.push_str("\n--- PREVIEW MODIFIED CONTENT ---\n");
        text.push_str(&entry.modified_src);
        text
    } else {
        "No .tm files found in directory.".to_string()
    };

    let detail = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Metadata & Source Preview "),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(detail, chunks[1]);
}

fn render_structural_view(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

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

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Structural Search Matches "),
    );
    f.render_widget(list, chunks[0]);

    // Diff Pane
    let diff_text = if let Some(m) = app.structural_matches.get(app.structural_index) {
        format!(
            "--- ORIGINAL SOURCE ({}) ---\n{}\n\n--- REFACTORED SOURCE ---\n{}",
            m.path.display(),
            m.original_src,
            m.modified_src
        )
    } else {
        "No matching elements found.".to_string()
    };

    let diff = Paragraph::new(diff_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Refactoring Diff Preview "),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(diff, chunks[1]);
}

fn render_controls(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.active_tab {
        ActiveTab::Migration => {
            "[Space] Toggle File | [a] Toggle All | [e] Convert Markdown to .tm | [q] Quit"
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
                "{tag_style}{} | {key_style}{} | {val_style}{} | {rep_style}{}  ([t]ag/[k]ey/[v]al/[r]eplace edit, [s]earch, [e] Apply Refactor)",
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
