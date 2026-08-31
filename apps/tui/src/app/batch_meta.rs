//! Batch Meta tab: bulk `@meta` key/value updates across selected
//! `.tmt` files.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use super::{App, PendingConfirm};
use tomet_indexer::extract_metadata;
use tomet_workspace::batch_meta::{BatchMetaEngine, MetaFile};

#[derive(Debug, Clone)]
pub struct MetaFileEntry {
    pub path: PathBuf,
    pub original_src: String,
    pub modified_src: String,
    pub metadata: BTreeMap<String, String>,
    pub selected: bool,
}

impl MetaFileEntry {
    pub fn from_path(path: PathBuf) -> Self {
        Self {
            path,
            original_src: String::new(),
            modified_src: String::new(),
            metadata: BTreeMap::new(),
            selected: true,
        }
    }

    pub fn ensure_loaded(&mut self) {
        if self.original_src.is_empty() {
            if let Ok(src) = fs::read_to_string(&self.path) {
                self.metadata = extract_metadata(&src);
                self.modified_src = src.clone();
                self.original_src = src;
            }
        }
    }
}

impl App {
    pub fn adjust_meta_scrolloff(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        let scrolloff = 2.min(height / 2);
        let total = self.meta_entries.len();
        if total == 0 {
            self.meta_scroll_offset = 0;
            return;
        }

        if self.meta_index < self.meta_scroll_offset + scrolloff {
            self.meta_scroll_offset = self.meta_index.saturating_sub(scrolloff);
        } else if self.meta_index + scrolloff >= self.meta_scroll_offset + height {
            self.meta_scroll_offset = (self.meta_index + scrolloff + 1).saturating_sub(height);
        }

        let max_offset = total.saturating_sub(height);
        if self.meta_scroll_offset > max_offset {
            self.meta_scroll_offset = max_offset;
        }
    }

    pub(super) fn batch_meta_move_up(&mut self) {
        if self.meta_index > 0 {
            self.meta_index -= 1;
        }
    }

    pub(super) fn batch_meta_move_down(&mut self) {
        if !self.meta_entries.is_empty() && self.meta_index < self.meta_entries.len() - 1 {
            self.meta_index += 1;
        }
    }

    pub(super) fn batch_meta_select_item(&mut self, relative_index: usize) {
        let actual = self.meta_scroll_offset + relative_index;
        if actual < self.meta_entries.len() {
            self.meta_index = actual;
        }
    }

    pub(super) fn batch_meta_toggle_select(&mut self) {
        if let Some(entry) = self.meta_entries.get_mut(self.meta_index) {
            entry.selected = !entry.selected;
        }
    }

    pub(super) fn batch_meta_toggle_select_all(&mut self) {
        let all_selected = self.meta_entries.iter().all(|i| i.selected);
        for e in &mut self.meta_entries {
            e.selected = !all_selected;
        }
    }

    pub(super) fn batch_meta_request_action(&mut self) {
        let count = self.meta_entries.iter().filter(|e| e.selected).count();
        if count == 0 {
            self.status_message =
                "No .tmt files selected! Press [Space] to select or [a] to select all.".to_string();
        } else if self.meta_key_input.is_empty() {
            self.status_message = "Please specify a metadata KEY to update.".to_string();
        } else {
            self.pending_confirm = Some(PendingConfirm {
                message: format!(
                    "Update meta field '{}' in {count} file(s)?",
                    self.meta_key_input
                ),
            });
        }
    }

    pub(super) fn batch_meta_execute_action(&mut self) {
        if !self.meta_key_input.is_empty() {
            let mut selected_files: Vec<MetaFile> = self
                .meta_entries
                .iter_mut()
                .filter(|e| e.selected)
                .map(|e| {
                    e.ensure_loaded();
                    MetaFile {
                        path: e.path.clone(),
                        original_src: e.original_src.clone(),
                        modified_src: e.modified_src.clone(),
                        metadata: e.metadata.clone(),
                    }
                })
                .collect();

            BatchMetaEngine::update_meta_key(
                &mut selected_files,
                "meta",
                &self.meta_key_input,
                &self.meta_val_input,
            );

            match BatchMetaEngine::save(&mut selected_files) {
                Ok(count) => {
                    for file in selected_files {
                        if let Some(entry) = self.meta_entries.iter_mut().find(|e| e.path == file.path) {
                            entry.original_src = file.original_src;
                            entry.modified_src = file.modified_src;
                            entry.metadata = file.metadata;
                        }
                    }
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

    pub(super) fn batch_meta_status_message(&self) -> String {
        let selected_count = self.meta_entries.iter().filter(|e| e.selected).count();
        format!(
            "Loaded {} .tmt file(s) ({} selected). Press [k]ey/[v]al to set metadata, [e] to apply.",
            self.meta_entries.len(),
            selected_count
        )
    }
}
