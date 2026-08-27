//! TypedMark Workbench TUI.

pub mod app;
pub mod engine;
pub mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::{App, FocusedPane, MetaEditTarget, StructuralInputTarget};

/// Launch the interactive TUI application.
pub fn run_tui(dir_path: PathBuf, config_path: Option<PathBuf>) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = match load_app_with_progress(&mut terminal, dir_path, config_path)? {
        Some(mut app) => run_app(&mut terminal, &mut app),
        None => Ok(()), // user quit while the workspace scan was still loading
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

/// Runs `App::new` (which walks the whole workspace, parsing every file it
/// finds) on a background thread, drawing an animated loading screen until
/// it finishes. Without this, the alternate screen stays blank for however
/// long the scan takes -- there's no other feedback that the app started.
/// Returns `Ok(None)` if the user quits (Ctrl+C/Esc) before the scan completes.
fn load_app_with_progress<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    dir_path: PathBuf,
    config_path: Option<PathBuf>,
) -> anyhow::Result<Option<App>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let app = App::new(dir_path, config_path);
        let _ = tx.send(app);
    });

    let mut tick: usize = 0;
    loop {
        terminal.draw(|f| ui::draw_loading(f, tick, "Scanning workspace..."))?;

        if event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = event::read()? {
                let is_ctrl_c = key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'));
                if is_ctrl_c || key.code == KeyCode::Esc {
                    return Ok(None);
                }
            }
        }

        match rx.recv_timeout(Duration::from_millis(80)) {
            Ok(app) => return Ok(Some(app)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("workspace scan thread panicked before finishing")
            }
        }

        tick = tick.wrapping_add(1);
    }
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if app.should_quit {
            return Ok(());
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            let term_size = terminal.size().unwrap_or_default();

            match event::read()? {
                Event::Mouse(mouse) => {
                    // Confirmation modal mouse handling
                    if app.pending_confirm.is_some() {
                        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                            app.confirm_action();
                        }
                        continue;
                    }

                    match mouse.kind {
                        MouseEventKind::ScrollUp => {
                            app.scroll_preview_up(3);
                        }
                        MouseEventKind::ScrollDown => {
                            app.scroll_preview_down(3);
                        }
                        MouseEventKind::Down(MouseButton::Left) => {
                            let x = mouse.column;
                            let y = mouse.row;

                            if y < 3 {
                                // Header Tabs
                                if x < 24 {
                                    app.active_tab = app::ActiveTab::Explorer;
                                } else if x < 50 {
                                    app.active_tab = app::ActiveTab::Migration;
                                } else if x < 74 {
                                    app.active_tab = app::ActiveTab::BatchMeta;
                                } else {
                                    app.active_tab = app::ActiveTab::StructuralGrep;
                                }
                                app.preview_scroll = 0;
                                app.refresh_status();
                            } else if y >= 3 && y < term_size.height.saturating_sub(4) {
                                let left_width = term_size.width * 40 / 100;
                                if x < left_width {
                                    app.focused_pane = FocusedPane::List;
                                    let clicked_row = (y.saturating_sub(4)) as usize;
                                    app.select_item(clicked_row);
                                } else {
                                    app.focused_pane = FocusedPane::Preview;
                                }
                            } else if y >= term_size.height.saturating_sub(4) {
                                app.focused_pane = FocusedPane::Controls;
                            }
                        }
                        _ => {}
                    }
                }
                Event::Key(key) => {
                    // Global Ctrl+C quit
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C'))
                    {
                        app.should_quit = true;
                        return Ok(());
                    }
                    // If confirmation modal is open
                    if app.pending_confirm.is_some() {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                                if app.inline_editor.is_some() {
                                    app.stop_inline_editor(true);
                                    app.pending_confirm = None;
                                } else {
                                    app.confirm_action();
                                }
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app.cancel_confirmation();
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // If editing in TUI inline editor
                    if app.inline_editor.is_some() {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && (key.code == KeyCode::Char('s') || key.code == KeyCode::Char('S'))
                        {
                            app.save_inline_editor();
                            continue;
                        }

                        let mut stop_edit = false;
                        if let Some(editor) = &mut app.inline_editor {
                            match key.code {
                                KeyCode::Esc => {
                                    stop_edit = true;
                                }
                                KeyCode::Enter => editor.insert_newline(),
                                KeyCode::Backspace => editor.backspace(),
                                KeyCode::Delete => editor.delete(),
                                KeyCode::Up => editor.move_up(),
                                KeyCode::Down => editor.move_down(),
                                KeyCode::Left => editor.move_left(),
                                KeyCode::Right => editor.move_right(),
                                KeyCode::Home => editor.move_home(),
                                KeyCode::End => editor.move_end(),
                                KeyCode::Char(c) => editor.insert_char(c),
                                _ => {}
                            }
                        }
                        if stop_edit {
                            app.stop_inline_editor(false);
                        }
                        continue;
                    }

                    // If typing in tree filter input
                    if app.is_filtering_tree {
                        match key.code {
                            KeyCode::Esc => {
                                if !app.tree_filter_input.is_empty() {
                                    app.tree_filter_input.clear();
                                } else {
                                    app.is_filtering_tree = false;
                                    app.focused_pane = FocusedPane::List;
                                }
                                app.tree_index = 0;
                            }
                            KeyCode::Enter => {
                                app.is_filtering_tree = false;
                                app.focused_pane = FocusedPane::List;
                            }
                            KeyCode::Backspace => {
                                app.tree_filter_input.pop();
                                app.tree_index = 0;
                            }
                            KeyCode::Char(c) => {
                                app.tree_filter_input.push(c);
                                app.tree_index = 0;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // If editing in input modes
                    if let Some(target) = app.meta_edit_target {
                        match key.code {
                            KeyCode::Esc => match target {
                                MetaEditTarget::Key if !app.meta_key_input.is_empty() => {
                                    app.meta_key_input.clear();
                                }
                                MetaEditTarget::Value if !app.meta_val_input.is_empty() => {
                                    app.meta_val_input.clear();
                                }
                                _ => {
                                    app.meta_edit_target = None;
                                    app.focused_pane = FocusedPane::List;
                                }
                            },
                            KeyCode::Enter => {
                                app.meta_edit_target = None;
                                app.focused_pane = FocusedPane::List;
                            }
                            KeyCode::Backspace => match target {
                                MetaEditTarget::Key => {
                                    app.meta_key_input.pop();
                                }
                                MetaEditTarget::Value => {
                                    app.meta_val_input.pop();
                                }
                            },
                            KeyCode::Char(c) => match target {
                                MetaEditTarget::Key => {
                                    app.meta_key_input.push(c);
                                }
                                MetaEditTarget::Value => {
                                    app.meta_val_input.push(c);
                                }
                            },
                            _ => {}
                        }
                        continue;
                    }

                    if let Some(target) = app.structural_input_target {
                        match key.code {
                            KeyCode::Esc => match target {
                                StructuralInputTarget::QueryTag
                                    if !app.query_tag_input.is_empty() =>
                                {
                                    app.query_tag_input.clear();
                                }
                                StructuralInputTarget::QueryKey
                                    if !app.query_key_input.is_empty() =>
                                {
                                    app.query_key_input.clear();
                                }
                                StructuralInputTarget::QueryVal
                                    if !app.query_val_input.is_empty() =>
                                {
                                    app.query_val_input.clear();
                                }
                                StructuralInputTarget::ReplaceTo
                                    if !app.replace_to_input.is_empty() =>
                                {
                                    app.replace_to_input.clear();
                                }
                                _ => {
                                    app.structural_input_target = None;
                                    app.focused_pane = FocusedPane::List;
                                }
                            },
                            KeyCode::Enter => {
                                app.structural_input_target = None;
                                app.focused_pane = FocusedPane::List;
                            }
                            KeyCode::Backspace => match target {
                                StructuralInputTarget::QueryTag => {
                                    app.query_tag_input.pop();
                                }
                                StructuralInputTarget::QueryKey => {
                                    app.query_key_input.pop();
                                }
                                StructuralInputTarget::QueryVal => {
                                    app.query_val_input.pop();
                                }
                                StructuralInputTarget::ReplaceTo => {
                                    app.replace_to_input.pop();
                                }
                            },
                            KeyCode::Char(c) => match target {
                                StructuralInputTarget::QueryTag => {
                                    app.query_tag_input.push(c);
                                }
                                StructuralInputTarget::QueryKey => {
                                    app.query_key_input.push(c);
                                }
                                StructuralInputTarget::QueryVal => {
                                    app.query_val_input.push(c);
                                }
                                StructuralInputTarget::ReplaceTo => {
                                    app.replace_to_input.push(c);
                                }
                            },
                            _ => {}
                        }
                        continue;
                    }

                    // Global Navigation & Commands
                    match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        }
                        KeyCode::Esc => {
                            if app.focused_pane != FocusedPane::List {
                                app.focused_pane = FocusedPane::List;
                            } else {
                                app.should_quit = true;
                            }
                        }
                        KeyCode::Tab => {
                            app.next_tab();
                        }
                        KeyCode::BackTab => {
                            app.previous_tab();
                        }
                        KeyCode::Char('1') => {
                            app.active_tab = app::ActiveTab::Explorer;
                            app.refresh_status();
                        }
                        KeyCode::Char('2') => {
                            app.active_tab = app::ActiveTab::Migration;
                            app.refresh_status();
                        }
                        KeyCode::Char('3') => {
                            app.active_tab = app::ActiveTab::BatchMeta;
                            app.refresh_status();
                        }
                        KeyCode::Char('4') => {
                            app.active_tab = app::ActiveTab::StructuralGrep;
                            app.refresh_status();
                        }
                        KeyCode::Left => match app.focused_pane {
                            FocusedPane::Preview | FocusedPane::Controls => {
                                app.focused_pane = FocusedPane::List;
                            }
                            FocusedPane::List => {
                                let mut handled = false;
                                let vis_idx = if app.active_tab == app::ActiveTab::Explorer {
                                    app.visible_tree_indices().get(app.tree_index).copied()
                                } else if app.active_tab == app::ActiveTab::Migration {
                                    app.visible_migration_tree_indices()
                                        .get(app.migration_index)
                                        .copied()
                                } else {
                                    None
                                };
                                if let Some(real_idx) = vis_idx {
                                    if let Some(node) = app.tree_nodes.get_mut(real_idx) {
                                        if node.is_dir && node.expanded {
                                            node.expanded = false;
                                            handled = true;
                                        }
                                    }
                                }
                                if !handled {
                                    app.previous_tab();
                                }
                            }
                        },
                        KeyCode::Right => match app.focused_pane {
                            FocusedPane::List => {
                                let mut handled = false;
                                let vis_idx = if app.active_tab == app::ActiveTab::Explorer {
                                    app.visible_tree_indices().get(app.tree_index).copied()
                                } else if app.active_tab == app::ActiveTab::Migration {
                                    app.visible_migration_tree_indices()
                                        .get(app.migration_index)
                                        .copied()
                                } else {
                                    None
                                };
                                if let Some(real_idx) = vis_idx {
                                    if let Some(node) = app.tree_nodes.get_mut(real_idx) {
                                        if node.is_dir && !node.expanded {
                                            node.expanded = true;
                                            handled = true;
                                        }
                                    }
                                }
                                if !handled {
                                    app.focused_pane = FocusedPane::Preview;
                                }
                            }
                            FocusedPane::Preview => {
                                app.focused_pane = FocusedPane::Controls;
                            }
                            FocusedPane::Controls => {
                                app.next_tab();
                            }
                        },
                        KeyCode::Up | KeyCode::Char('k') if key.modifiers.is_empty() => {
                            match app.focused_pane {
                                FocusedPane::List => app.move_up(),
                                FocusedPane::Preview => app.scroll_preview_up(3),
                                FocusedPane::Controls => app.focused_pane = FocusedPane::List,
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') if key.modifiers.is_empty() => {
                            match app.focused_pane {
                                FocusedPane::List => app.move_down(),
                                FocusedPane::Preview => app.scroll_preview_down(3),
                                FocusedPane::Controls => app.move_down(),
                            }
                        }
                        KeyCode::PageDown => {
                            app.scroll_preview_down(5);
                        }
                        KeyCode::PageUp => {
                            app.scroll_preview_up(5);
                        }
                        KeyCode::Char(' ') => {
                            app.toggle_select();
                        }
                        KeyCode::Char('a') => {
                            app.toggle_select_all();
                        }
                        KeyCode::Enter => {
                            if app.active_tab == app::ActiveTab::Explorer
                                || app.active_tab == app::ActiveTab::Migration
                            {
                                app.toggle_select();
                            } else {
                                app.request_execute_action();
                            }
                        }
                        KeyCode::Char('e') => {
                            if app.active_tab == app::ActiveTab::Explorer {
                                app.start_inline_editor();
                            } else {
                                app.request_execute_action();
                            }
                        }
                        KeyCode::Char('E') => {
                            if app.active_tab == app::ActiveTab::Explorer {
                                if let Some(path) = app.get_selected_file_path() {
                                    let editor = std::env::var("EDITOR")
                                        .or_else(|_| std::env::var("VISUAL"))
                                        .unwrap_or_else(|_| "nano".to_string());

                                    let mut stdout = std::io::stdout();
                                    disable_raw_mode()?;
                                    execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;

                                    let _ = std::process::Command::new(&editor).arg(&path).status();

                                    enable_raw_mode()?;
                                    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
                                    terminal.clear()?;
                                    app.reload_workspace();
                                }
                            } else {
                                app.request_execute_action();
                            }
                        }

                        KeyCode::Char('/') if app.active_tab == app::ActiveTab::Explorer => {
                            app.is_filtering_tree = true;
                            app.focused_pane = FocusedPane::Controls;
                        }

                        // Tab specific input toggles
                        KeyCode::Char('k') if app.active_tab == app::ActiveTab::BatchMeta => {
                            app.meta_edit_target = Some(MetaEditTarget::Key);
                            app.focused_pane = FocusedPane::Controls;
                        }
                        KeyCode::Char('v') if app.active_tab == app::ActiveTab::BatchMeta => {
                            app.meta_edit_target = Some(MetaEditTarget::Value);
                            app.focused_pane = FocusedPane::Controls;
                        }
                        KeyCode::Char('t') if app.active_tab == app::ActiveTab::StructuralGrep => {
                            app.structural_input_target = Some(StructuralInputTarget::QueryTag);
                            app.focused_pane = FocusedPane::Controls;
                        }
                        KeyCode::Char('k') if app.active_tab == app::ActiveTab::StructuralGrep => {
                            app.structural_input_target = Some(StructuralInputTarget::QueryKey);
                            app.focused_pane = FocusedPane::Controls;
                        }
                        KeyCode::Char('v') if app.active_tab == app::ActiveTab::StructuralGrep => {
                            app.structural_input_target = Some(StructuralInputTarget::QueryVal);
                            app.focused_pane = FocusedPane::Controls;
                        }
                        KeyCode::Char('s') if app.active_tab == app::ActiveTab::StructuralGrep => {
                            app.run_structural_query();
                        }
                        KeyCode::Char('r') if app.active_tab == app::ActiveTab::StructuralGrep => {
                            app.structural_input_target = Some(StructuralInputTarget::ReplaceTo);
                            app.focused_pane = FocusedPane::Controls;
                        }
                        KeyCode::Char('c') if app.active_tab == app::ActiveTab::StructuralGrep => {
                            app.clear_structural_inputs();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}
