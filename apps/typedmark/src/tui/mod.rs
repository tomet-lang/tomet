//! TypedMark Workbench TUI module.

pub mod app;
pub mod engine;
pub mod ui;

use std::io;
use std::path::PathBuf;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, MetaEditTarget, StructuralInputTarget};

/// Launch the interactive TUI application.
pub fn run_tui(dir_path: PathBuf) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(dir_path);
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
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
            if let Event::Key(key) = event::read()? {
                // If editing in input modes
                if let Some(target) = app.meta_edit_target {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            app.meta_edit_target = None;
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
                        KeyCode::Esc | KeyCode::Enter => {
                            app.structural_input_target = None;
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
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Tab => {
                        app.next_tab();
                    }
                    KeyCode::BackTab => {
                        app.previous_tab();
                    }
                    KeyCode::Char('1') => {
                        app.active_tab = app::ActiveTab::Migration;
                        app.refresh_status();
                    }
                    KeyCode::Char('2') => {
                        app.active_tab = app::ActiveTab::BatchMeta;
                        app.refresh_status();
                    }
                    KeyCode::Char('3') => {
                        app.active_tab = app::ActiveTab::StructuralGrep;
                        app.refresh_status();
                    }
                    KeyCode::Up | KeyCode::Char('k') if key.modifiers.is_empty() => {
                        app.move_up();
                    }
                    KeyCode::Down | KeyCode::Char('j') if key.modifiers.is_empty() => {
                        app.move_down();
                    }
                    KeyCode::Char(' ') => {
                        app.toggle_select();
                    }
                    KeyCode::Char('a') => {
                        app.toggle_select_all();
                    }
                    KeyCode::Char('e') | KeyCode::Enter => {
                        app.execute_action();
                    }

                    // Tab specific input toggles
                    KeyCode::Char('k') if app.active_tab == app::ActiveTab::BatchMeta => {
                        app.meta_edit_target = Some(MetaEditTarget::Key);
                    }
                    KeyCode::Char('v') if app.active_tab == app::ActiveTab::BatchMeta => {
                        app.meta_edit_target = Some(MetaEditTarget::Value);
                    }
                    KeyCode::Char('t') if app.active_tab == app::ActiveTab::StructuralGrep => {
                        app.structural_input_target = Some(StructuralInputTarget::QueryTag);
                    }
                    KeyCode::Char('k') if app.active_tab == app::ActiveTab::StructuralGrep => {
                        app.structural_input_target = Some(StructuralInputTarget::QueryKey);
                    }
                    KeyCode::Char('s') if app.active_tab == app::ActiveTab::StructuralGrep => {
                        app.run_structural_query();
                    }
                    KeyCode::Char('r') if app.active_tab == app::ActiveTab::StructuralGrep => {
                        app.structural_input_target = Some(StructuralInputTarget::ReplaceTo);
                    }
                    _ => {}
                }
            }
        }
    }
}
