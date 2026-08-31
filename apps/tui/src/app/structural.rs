//! Structural Grep tab: AST-aware search & replace (rename tag, rename
//! key, replace value) across `.tmt` files.

use std::path::PathBuf;

use super::{App, PendingConfirm};
use tomet_transform::{StructuralAction, StructuralQuery};
use tomet_workspace::diff::FileDiff;
use tomet_workspace::structural::StructuralEngine;

#[derive(Debug, Clone)]
pub struct StructuralMatch {
    pub path: PathBuf,
    pub original_src: String,
    pub modified_src: String,
    pub match_count: usize,
    pub selected: bool,
}

impl StructuralMatch {
    pub fn from_diff(diff: FileDiff) -> Self {
        Self {
            path: diff.path,
            original_src: diff.original_src,
            modified_src: diff.modified_src,
            match_count: diff.changes_count,
            selected: true,
        }
    }
}

impl App {
    pub fn adjust_structural_scrolloff(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        let scrolloff = 2.min(height / 2);
        let total = self.structural_matches.len();
        if total == 0 {
            self.structural_scroll_offset = 0;
            return;
        }

        if self.structural_index < self.structural_scroll_offset + scrolloff {
            self.structural_scroll_offset = self.structural_index.saturating_sub(scrolloff);
        } else if self.structural_index + scrolloff >= self.structural_scroll_offset + height {
            self.structural_scroll_offset =
                (self.structural_index + scrolloff + 1).saturating_sub(height);
        }

        let max_offset = total.saturating_sub(height);
        if self.structural_scroll_offset > max_offset {
            self.structural_scroll_offset = max_offset;
        }
    }

    pub fn clear_structural_inputs(&mut self) {
        self.query_tag_input.clear();
        self.query_key_input.clear();
        self.query_val_input.clear();
        self.replace_to_input.clear();
        self.structural_input_target = None;
        self.run_structural_query();
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
        let diffs = StructuralEngine::search_with_config(
            &self.dir_path,
            &query,
            &self.printer_config,
            &self.config_root,
        );
        self.structural_matches = diffs.into_iter().map(StructuralMatch::from_diff).collect();
        self.structural_index = 0;
        self.status_message = format!("Found {} matching file(s).", self.structural_matches.len());
    }

    pub(super) fn structural_move_up(&mut self) {
        if self.structural_index > 0 {
            self.structural_index -= 1;
        }
    }

    pub(super) fn structural_move_down(&mut self) {
        if !self.structural_matches.is_empty()
            && self.structural_index < self.structural_matches.len() - 1
        {
            self.structural_index += 1;
        }
    }

    pub(super) fn structural_select_item(&mut self, relative_index: usize) {
        let actual = self.structural_scroll_offset + relative_index;
        if actual < self.structural_matches.len() {
            self.structural_index = actual;
        }
    }

    pub(super) fn structural_toggle_select(&mut self) {
        if let Some(m) = self.structural_matches.get_mut(self.structural_index) {
            m.selected = !m.selected;
        }
    }

    pub(super) fn structural_toggle_select_all(&mut self) {
        let all_selected = self.structural_matches.iter().all(|m| m.selected);
        for m in &mut self.structural_matches {
            m.selected = !all_selected;
        }
    }

    pub(super) fn structural_request_action(&mut self) {
        let count = self
            .structural_matches
            .iter()
            .filter(|m| m.selected)
            .count();
        if count == 0 {
            self.status_message =
                "No matches selected! Press [Space] to select or [a] to select all.".to_string();
        } else if self.replace_to_input.is_empty() {
            self.status_message = "Please specify a REPLACE target value.".to_string();
        } else {
            self.pending_confirm = Some(PendingConfirm {
                message: format!("Apply refactoring to {count} file(s)?"),
            });
        }
    }

    pub(super) fn structural_execute_action(&mut self) {
        let action = if !self.query_key_input.is_empty() && !self.replace_to_input.is_empty() {
            if !self.query_val_input.is_empty() {
                Some(StructuralAction::ReplaceValue {
                    key: self.query_key_input.clone(),
                    new_value: self.replace_to_input.clone(),
                })
            } else {
                Some(StructuralAction::RenameKey {
                    old_key: self.query_key_input.clone(),
                    new_key: self.replace_to_input.clone(),
                })
            }
        } else if !self.query_tag_input.is_empty() && !self.replace_to_input.is_empty() {
            Some(StructuralAction::RenameTag {
                from: self.query_tag_input.clone(),
                to: self.replace_to_input.clone(),
            })
        } else {
            None
        };

        if let Some(act) = action {
            let mut diffs: Vec<FileDiff> = self
                .structural_matches
                .iter()
                .filter(|m| m.selected)
                .map(|m| {
                    FileDiff::new(
                        m.path.clone(),
                        m.original_src.clone(),
                        m.modified_src.clone(),
                        m.match_count,
                    )
                })
                .collect();

            StructuralEngine::apply_action(&mut diffs, &act);

            match tomet_workspace::save_file_diffs(&mut diffs) {
                Ok(count) => {
                    for diff in diffs {
                        if let Some(m) = self
                            .structural_matches
                            .iter_mut()
                            .find(|m| m.path == diff.path)
                        {
                            m.original_src = diff.original_src;
                            m.modified_src = diff.modified_src;
                            m.match_count = diff.changes_count;
                        }
                    }
                    self.status_message = format!("Refactored {count} file(s).");
                }
                Err(e) => {
                    self.status_message = format!("Refactor save error: {e}");
                }
            }
        }
    }

    pub(super) fn structural_status_message(&self) -> String {
        let selected_count = self
            .structural_matches
            .iter()
            .filter(|m| m.selected)
            .count();
        format!(
            "Matched {} file(s) ({} selected). Press [t]/[k]/[v]/[r] field, [s]earch, [e] to refactor.",
            self.structural_matches.len(),
            selected_count
        )
    }
}
