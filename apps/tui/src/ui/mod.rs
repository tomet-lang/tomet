//! Ratatui UI Layout and widget rendering for Tomet Workbench TUI.
//!
//! `draw` lays out the shared chrome (header/tabs, controls bar,
//! footer, confirm modal -- this file) and delegates the main content
//! area to a per-tab `render_*_view` function in this module's
//! submodules (`explorer`/`migration`/`batch_meta`/`structural`).
//! `render_controls` similarly delegates its per-tab hint text to each
//! submodule's `*_controls_info`. `diff`/`highlight` hold
//! tab-independent utilities (before/after diffing, `.tmt` syntax
//! highlighting) shared across the tab-specific view functions.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs},
};

use crate::app::{ActiveTab, App, PendingConfirm};

mod batch_meta;
mod diff;
mod explorer;
mod highlight;
mod migration;
mod structural;

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
        ActiveTab::Explorer => explorer::render_explorer_view(f, app, chunks[1]),
        ActiveTab::Migration => migration::render_migration_view(f, app, chunks[1]),
        ActiveTab::BatchMeta => batch_meta::render_batch_meta_view(f, app, chunks[1]),
        ActiveTab::StructuralGrep => structural::render_structural_view(f, app, chunks[1]),
    }

    render_controls(f, app, chunks[2]);
    render_footer(f, app, chunks[3]);

    if let Some(confirm) = &app.pending_confirm {
        render_confirm_modal(f, confirm, f.area());
    }
}

/// Animated placeholder screen shown while the workspace scan (`App::new`)
/// runs on a background thread. `tick` is a monotonically increasing frame
/// counter driven by the caller's poll loop -- there's no real progress
/// percentage to report (the scan doesn't know the file count up front),
/// so this renders a spinner plus a sweeping indeterminate bar instead.
pub fn draw_loading(f: &mut Frame, tick: usize, message: &str) {
    let area = f.area();

    let width = 54.min(area.width.saturating_sub(4)).max(20);
    let height = 5.min(area.height.saturating_sub(2)).max(4);
    let popup_area = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tomet Workbench TUI ")
        .border_style(Style::default().fg(Color::Cyan));

    const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner = SPINNER[tick % SPINNER.len()];

    let bar_width = (width as usize).saturating_sub(4).max(6);
    let segment_len = (bar_width / 4).max(1);
    let period = bar_width + segment_len;
    let pos = tick % period;

    let bar: String = (0..bar_width)
        .map(|i| {
            if i >= pos.saturating_sub(segment_len) && i < pos {
                '█'
            } else {
                '░'
            }
        })
        .collect();

    let text = vec![
        Line::from(Span::styled(
            format!("  {spinner} {message}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!("  {bar}"),
            Style::default().fg(Color::Cyan),
        )),
    ];

    let p = Paragraph::new(text).block(block);
    f.render_widget(p, popup_area);
}

fn get_border_style(is_focused: bool) -> Style {
    if is_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
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
        format!(" Tomet Workbench TUI (Config: {}) ", path.display())
    } else {
        " Tomet Workbench TUI (Config: default) ".to_string()
    };

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(config_title))
        .select(app.active_tab as usize)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

fn render_controls(f: &mut Frame, app: &App, area: Rect) {
    let info = match app.active_tab {
        ActiveTab::Explorer => explorer::explorer_controls_info(app),
        ActiveTab::Migration => migration::migration_controls_info(app),
        ActiveTab::BatchMeta => batch_meta::batch_meta_controls_info(app),
        ActiveTab::StructuralGrep => structural::structural_controls_info(app),
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
