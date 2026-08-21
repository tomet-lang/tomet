//! Migration tab: selecting Markdown files/directories to convert to
//! `.tm` and running the conversion.

use std::path::Path;

use super::{App, PendingConfirm};
use crate::engine::migration::MigrationEngine;
use typedmark_edit::batch_meta::BatchMetaEngine;

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
        let dir_depth = self.tree_nodes[real_index].depth;
        let is_dir = self.tree_nodes[real_index].is_dir;

        if is_dir {
            let mut all_selected = true;
            let mut count = 0;

            for desc in &self.tree_nodes[(real_index + 1)..] {
                if desc.depth <= dir_depth {
                    break;
                }
                if let Some(item) = &desc.migration_item {
                    count += 1;
                    if !item.selected {
                        all_selected = false;
                    }
                }
            }

            let new_state = if count > 0 { !all_selected } else { true };
            let node_path = self.tree_nodes[real_index].path.clone();

            for desc in &mut self.tree_nodes[real_index..] {
                if desc.depth <= dir_depth && desc.path != node_path {
                    break;
                }
                if let Some(item) = desc.migration_item.as_mut() {
                    item.selected = new_state;
                }
            }
            for item in &mut self.migration_items {
                if item.source_path.starts_with(&node_path) {
                    item.selected = new_state;
                }
            }
        } else {
            let node_path = self.tree_nodes[real_index].path.clone();
            if let Some(item) = self.tree_nodes[real_index].migration_item.as_mut() {
                item.selected = !item.selected;
            }
            if let Some(item) = self
                .migration_items
                .iter_mut()
                .find(|i| i.source_path == node_path)
            {
                item.selected = !item.selected;
            }
        }
    }

    pub fn get_dir_migration_status_for_node(&self, real_index: usize) -> (usize, usize) {
        let dir_node = &self.tree_nodes[real_index];
        let mut total = 0;
        let mut selected = 0;

        for desc in &self.tree_nodes[(real_index + 1)..] {
            if desc.depth <= dir_node.depth {
                break;
            }
            if let Some(item) = &desc.migration_item {
                total += 1;
                if item.selected {
                    selected += 1;
                }
            }
        }
        (selected, total)
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
                message: format!("Convert {count} Markdown file(s) to .tm?"),
            });
        }
    }

    pub(super) fn migration_execute_action(&mut self) {
        match MigrationEngine::execute_with_config(
            &mut self.migration_items,
            false,
            &self.printer_config,
        ) {
            Ok(count) => {
                self.status_message =
                    format!("Successfully converted {count} Markdown file(s) to .tm!");
                self.tree_nodes = MigrationEngine::scan_tree_with_config(
                    &self.dir_path,
                    &self.printer_config,
                    &self.config_root,
                );
                self.meta_entries = BatchMetaEngine::scan_with_config(
                    &self.dir_path,
                    &self.printer_config,
                    &self.config_root,
                );
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
