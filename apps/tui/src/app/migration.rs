//! Migration tab: selecting Markdown files/directories to convert to
//! `.tmt` and running the conversion.

use std::path::Path;

use super::{App, PendingConfirm};
use crate::engine::migration::MigrationEngine;

impl App {
    pub fn visible_migration_tree_indices(&self) -> Vec<usize> {
        let mut visible = Vec::new();
        let mut expanded_stack: Vec<bool> = vec![true];

        for (i, node) in self.tree_nodes.iter().enumerate() {
            expanded_stack.truncate(node.depth + 1);
            let parent_expanded = expanded_stack.last().copied().unwrap_or(true);

            if parent_expanded && node.has_md_children {
                visible.push(i);
            }

            if node.is_dir {
                expanded_stack.push(parent_expanded && node.expanded);
            }
        }
        visible
    }

    pub fn toggle_migration_node(&mut self, real_index: usize) {
        if real_index >= self.tree_nodes.len() {
            return;
        }
        let node_path = self.tree_nodes[real_index].path.clone();

        if self.tree_nodes[real_index].is_dir {
            let (selected, total) = self.get_dir_migration_status(&node_path);
            let new_state = if total > 0 { selected < total } else { true };
            for item in &mut self.migration_items {
                if item.source_path.starts_with(&node_path) {
                    item.selected = new_state;
                }
            }
        } else if let Some(item) = self
            .migration_items
            .iter_mut()
            .find(|i| i.source_path == node_path)
        {
            item.selected = !item.selected;
        }
    }

    pub fn get_dir_migration_status_for_node(&self, real_index: usize) -> (usize, usize) {
        self.get_dir_migration_status(&self.tree_nodes[real_index].path)
    }

    pub fn get_dir_migration_status(&self, dir_path: &Path) -> (usize, usize) {
        let mut total = 0;
        let mut selected = 0;
        for item in &self.migration_items {
            if item.source_path.starts_with(dir_path) {
                total += 1;
                if item.selected {
                    selected += 1;
                }
            }
        }
        (selected, total)
    }

    pub fn adjust_migration_scrolloff(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        let scrolloff = 2.min(height / 2);
        let vis_count = self.visible_migration_tree_indices().len();
        if vis_count == 0 {
            self.migration_scroll_offset = 0;
            return;
        }

        if self.migration_index >= vis_count {
            self.migration_index = vis_count.saturating_sub(1);
        }

        if self.migration_index < self.migration_scroll_offset + scrolloff {
            self.migration_scroll_offset = self.migration_index.saturating_sub(scrolloff);
        } else if self.migration_index + scrolloff >= self.migration_scroll_offset + height {
            self.migration_scroll_offset =
                (self.migration_index + scrolloff + 1).saturating_sub(height);
        }

        let max_offset = vis_count.saturating_sub(height);
        if self.migration_scroll_offset > max_offset {
            self.migration_scroll_offset = max_offset;
        }
    }

    pub(super) fn migration_move_up(&mut self) {
        if self.migration_index > 0 {
            self.migration_index -= 1;
        }
    }

    pub(super) fn migration_move_down(&mut self) {
        let vis_count = self.visible_migration_tree_indices().len();
        if vis_count > 0 && self.migration_index < vis_count - 1 {
            self.migration_index += 1;
        }
    }

    pub(super) fn migration_select_item(&mut self, relative_index: usize) {
        let actual = self.migration_scroll_offset + relative_index;
        let vis = self.visible_migration_tree_indices();
        if actual < vis.len() {
            self.migration_index = actual;
        }
    }

    pub(super) fn migration_toggle_select(&mut self) {
        let vis = self.visible_migration_tree_indices();
        if let Some(&real_idx) = vis.get(self.migration_index) {
            if self.tree_nodes[real_idx].is_dir {
                self.tree_nodes[real_idx].expanded = !self.tree_nodes[real_idx].expanded;
            }
            self.toggle_migration_node(real_idx);
        }
        self.refresh_status();
    }

    pub(super) fn migration_toggle_select_all(&mut self) {
        let all_selected = self.migration_items.iter().all(|i| i.selected);
        for i in &mut self.migration_items {
            i.selected = !all_selected;
        }
        self.refresh_status();
    }

    pub(super) fn migration_request_action(&mut self) {
        let count = self
            .migration_items
            .iter()
            .filter(|i| i.selected && !i.converted)
            .count();
        if count == 0 {
            self.status_message =
                "No Markdown files selected! Press [Space] to select or [a] to select all."
                    .to_string();
        } else {
            self.pending_confirm = Some(PendingConfirm {
                message: format!("Convert {count} Markdown file(s) to .tmt?"),
            });
        }
    }

    pub(super) fn migration_execute_action(&mut self) {
        let touched: Vec<(std::path::PathBuf, std::path::PathBuf)> = self
            .migration_items
            .iter()
            .filter(|i| i.selected && !i.converted)
            .map(|i| (i.source_path.clone(), i.target_path.clone()))
            .collect();

        match MigrationEngine::execute_with_config(
            &mut self.migration_items,
            false,
            &self.printer_config,
        ) {
            Ok(count) => {
                self.status_message =
                    format!("Successfully converted {count} Markdown file(s) to .tmt!");
                for (source, target) in &touched {
                    self.index.refresh_path(source);
                    self.index.refresh_path(target);
                }
                self.sync_workspace_views();
            }
            Err(e) => {
                self.status_message = format!("Migration error: {e}");
            }
        }
    }

    pub(super) fn migration_status_message(&self) -> String {
        let selected_count = self
            .migration_items
            .iter()
            .filter(|i| i.selected && !i.converted)
            .count();
        format!(
            "Found {} Markdown file(s) ({} selected). Press [Space] to toggle, [a] toggle all, [e] to convert.",
            self.migration_items.len(),
            selected_count
        )
    }
}
