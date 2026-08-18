//! State management for the TypedMark Workbench TUI.

use std::path::PathBuf;

use super::engine::batch_meta::{BatchMetaEngine, MetaFileEntry};
use super::engine::migration::{MigrationEngine, MigrationItem};
use super::engine::structural::{
    StructuralAction, StructuralEngine, StructuralMatch, StructuralQuery,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Migration = 0,
    BatchMeta = 1,
    StructuralGrep = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaEditTarget {
    Key,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralInputTarget {
    QueryTag,
    QueryKey,
    QueryVal,
    ReplaceTo,
}

pub struct App {
    pub dir_path: PathBuf,
    pub active_tab: ActiveTab,
    pub status_message: String,
    pub should_quit: bool,

    // Tab 1: Migration State
    pub migration_items: Vec<MigrationItem>,
    pub migration_index: usize,

    // Tab 2: Batch Meta State
    pub meta_entries: Vec<MetaFileEntry>,
    pub meta_index: usize,
    pub meta_key_input: String,
    pub meta_val_input: String,
    pub meta_edit_target: Option<MetaEditTarget>,

    // Tab 3: Structural Grep State
    pub structural_matches: Vec<StructuralMatch>,
    pub structural_index: usize,
    pub query_tag_input: String,
    pub query_key_input: String,
    pub query_val_input: String,
    pub replace_to_input: String,
    pub structural_input_target: Option<StructuralInputTarget>,
}

impl App {
    pub fn new(dir_path: PathBuf) -> Self {
        let migration_items = MigrationEngine::scan(&dir_path);
        let meta_entries = BatchMetaEngine::scan(&dir_path);
        let structural_matches = StructuralEngine::search(&dir_path, &StructuralQuery::default());

        let mut app = Self {
            dir_path,
            active_tab: ActiveTab::Migration,
            status_message: "Welcome to TypedMark Workbench! Press Tab/1/2/3 to switch views."
                .to_string(),
            should_quit: false,

            migration_items,
            migration_index: 0,

            meta_entries,
            meta_index: 0,
            meta_key_input: "author".to_string(),
            meta_val_input: "".to_string(),
            meta_edit_target: None,

            structural_matches,
            structural_index: 0,
            query_tag_input: "".to_string(),
            query_key_input: "".to_string(),
            query_val_input: "".to_string(),
            replace_to_input: "".to_string(),
            structural_input_target: None,
        };

        app.refresh_status();
        app
    }

    pub fn refresh_status(&mut self) {
        match self.active_tab {
            ActiveTab::Migration => {
                self.status_message = format!(
                    "Found {} Markdown file(s). Press [Space] to toggle, [e] to convert to .tm.",
                    self.migration_items.len()
                );
            }
            ActiveTab::BatchMeta => {
                self.status_message = format!(
                    "Loaded {} .tm file(s). Press [k]ey/[v]al to set metadata, [e] to apply.",
                    self.meta_entries.len()
                );
            }
            ActiveTab::StructuralGrep => {
                self.status_message = format!(
                    "Matched {} file(s). Press [t]ag/[k]ey/[s]earch/[r]eplace field, [e] to refactor.",
                    self.structural_matches.len()
                );
            }
        }
    }

    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Migration => ActiveTab::BatchMeta,
            ActiveTab::BatchMeta => ActiveTab::StructuralGrep,
            ActiveTab::StructuralGrep => ActiveTab::Migration,
        };
        self.refresh_status();
    }

    pub fn previous_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Migration => ActiveTab::StructuralGrep,
            ActiveTab::BatchMeta => ActiveTab::Migration,
            ActiveTab::StructuralGrep => ActiveTab::BatchMeta,
        };
        self.refresh_status();
    }

    pub fn move_up(&mut self) {
        match self.active_tab {
            ActiveTab::Migration => {
                if self.migration_index > 0 {
                    self.migration_index -= 1;
                }
            }
            ActiveTab::BatchMeta => {
                if self.meta_index > 0 {
                    self.meta_index -= 1;
                }
            }
            ActiveTab::StructuralGrep => {
                if self.structural_index > 0 {
                    self.structural_index -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.active_tab {
            ActiveTab::Migration => {
                if !self.migration_items.is_empty()
                    && self.migration_index < self.migration_items.len() - 1
                {
                    self.migration_index += 1;
                }
            }
            ActiveTab::BatchMeta => {
                if !self.meta_entries.is_empty() && self.meta_index < self.meta_entries.len() - 1 {
                    self.meta_index += 1;
                }
            }
            ActiveTab::StructuralGrep => {
                if !self.structural_matches.is_empty()
                    && self.structural_index < self.structural_matches.len() - 1
                {
                    self.structural_index += 1;
                }
            }
        }
    }

    pub fn toggle_select(&mut self) {
        match self.active_tab {
            ActiveTab::Migration => {
                if let Some(item) = self.migration_items.get_mut(self.migration_index) {
                    item.selected = !item.selected;
                }
            }
            ActiveTab::BatchMeta => {
                if let Some(entry) = self.meta_entries.get_mut(self.meta_index) {
                    entry.selected = !entry.selected;
                }
            }
            ActiveTab::StructuralGrep => {
                if let Some(m) = self.structural_matches.get_mut(self.structural_index) {
                    m.selected = !m.selected;
                }
            }
        }
    }

    pub fn toggle_select_all(&mut self) {
        match self.active_tab {
            ActiveTab::Migration => {
                let all_selected = self.migration_items.iter().all(|i| i.selected);
                for i in &mut self.migration_items {
                    i.selected = !all_selected;
                }
            }
            ActiveTab::BatchMeta => {
                let all_selected = self.meta_entries.iter().all(|i| i.selected);
                for e in &mut self.meta_entries {
                    e.selected = !all_selected;
                }
            }
            ActiveTab::StructuralGrep => {
                let all_selected = self.structural_matches.iter().all(|m| m.selected);
                for m in &mut self.structural_matches {
                    m.selected = !all_selected;
                }
            }
        }
    }

    pub fn execute_action(&mut self) {
        match self.active_tab {
            ActiveTab::Migration => {
                match MigrationEngine::execute(&mut self.migration_items, false) {
                    Ok(count) => {
                        self.status_message =
                            format!("Successfully converted {count} Markdown file(s) to .tm!");
                        // Re-scan meta entries to reflect newly generated .tm files
                        self.meta_entries = BatchMetaEngine::scan(&self.dir_path);
                    }
                    Err(e) => {
                        self.status_message = format!("Migration error: {e}");
                    }
                }
            }
            ActiveTab::BatchMeta => {
                if !self.meta_key_input.is_empty() {
                    BatchMetaEngine::update_meta_key(
                        &mut self.meta_entries,
                        "meta",
                        &self.meta_key_input,
                        &self.meta_val_input,
                    );
                    match BatchMetaEngine::save(&mut self.meta_entries) {
                        Ok(count) => {
                            self.status_message = format!(
                                "Updated meta field '{}' in {count} file(s).",
                                self.meta_key_input
                            );
                        }
                        Err(e) => {
                            self.status_message = format!("Save error: {e}");
                        }
                    }
                }
            }
            ActiveTab::StructuralGrep => {
                if !self.query_key_input.is_empty() && !self.replace_to_input.is_empty() {
                    StructuralEngine::apply_action(
                        &mut self.structural_matches,
                        &StructuralAction::RenameKey {
                            old_key: self.query_key_input.clone(),
                            new_key: self.replace_to_input.clone(),
                        },
                    );
                } else if !self.query_tag_input.is_empty() && !self.replace_to_input.is_empty() {
                    StructuralEngine::apply_action(
                        &mut self.structural_matches,
                        &StructuralAction::RenameTag {
                            from: self.query_tag_input.clone(),
                            to: self.replace_to_input.clone(),
                        },
                    );
                }
                match StructuralEngine::save(&mut self.structural_matches) {
                    Ok(count) => {
                        self.status_message = format!("Refactored {count} file(s).");
                    }
                    Err(e) => {
                        self.status_message = format!("Refactor save error: {e}");
                    }
                }
            }
        }
    }

    pub fn run_structural_query(&mut self) {
        let query = StructuralQuery {
            tag: if self.query_tag_input.is_empty() {
                None
            } else {
                Some(self.query_tag_input.clone())
            },
            key: if self.query_key_input.is_empty() {
                None
            } else {
                Some(self.query_key_input.clone())
            },
            value_contains: if self.query_val_input.is_empty() {
                None
            } else {
                Some(self.query_val_input.clone())
            },
        };
        self.structural_matches = StructuralEngine::search(&self.dir_path, &query);
        self.structural_index = 0;
        self.status_message = format!("Found {} matching file(s).", self.structural_matches.len());
    }
}
