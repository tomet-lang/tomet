//! Explorer tab: browsing/filtering the workspace file tree and
//! opening the inline editor.

use std::path::PathBuf;

use super::App;

impl App {
    pub fn visible_tree_indices(&self) -> Vec<usize> {
        let filter = self.tree_filter_input.trim().to_lowercase();
        let mut visible = Vec::new();
        let mut expanded_stack: Vec<bool> = vec![true];

        for (i, node) in self.tree_nodes.iter().enumerate() {
            expanded_stack.truncate(node.depth + 1);
            let parent_expanded = expanded_stack.last().copied().unwrap_or(true);

            if filter.is_empty() {
                if parent_expanded {
                    visible.push(i);
                }
            } else {
                if node.name.to_lowercase().contains(&filter)
                    || node
                        .path
                        .display()
                        .to_string()
                        .to_lowercase()
                        .contains(&filter)
                {
                    visible.push(i);
                }
            }

            if node.is_dir {
                expanded_stack.push(parent_expanded && node.expanded);
            }
        }
        visible
    }

    pub fn adjust_tree_scrolloff(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        let scrolloff = 2.min(height / 2);
        let vis_count = self.visible_tree_indices().len();
        if vis_count == 0 {
            self.tree_scroll_offset = 0;
            return;
        }

        if self.tree_index < self.tree_scroll_offset + scrolloff {
            self.tree_scroll_offset = self.tree_index.saturating_sub(scrolloff);
        } else if self.tree_index + scrolloff >= self.tree_scroll_offset + height {
            self.tree_scroll_offset = (self.tree_index + scrolloff + 1).saturating_sub(height);
        }

        let max_offset = vis_count.saturating_sub(height);
        if self.tree_scroll_offset > max_offset {
            self.tree_scroll_offset = max_offset;
        }
    }

    pub fn get_selected_file_path(&self) -> Option<PathBuf> {
        if self.active_tab != super::ActiveTab::Explorer {
            return None;
        }
        let vis = self.visible_tree_indices();
        if let Some(&real_idx) = vis.get(self.tree_index)
            && let Some(node) = self.tree_nodes.get(real_idx)
            && !node.is_dir
        {
            return Some(node.path.clone());
        }
        None
    }

    pub(super) fn explorer_move_up(&mut self) {
        if self.tree_index > 0 {
            self.tree_index -= 1;
        }
    }

    pub(super) fn explorer_move_down(&mut self) {
        let vis_count = self.visible_tree_indices().len();
        if vis_count > 0 && self.tree_index < vis_count - 1 {
            self.tree_index += 1;
        }
    }

    pub(super) fn explorer_select_item(&mut self, relative_index: usize) {
        let actual = self.tree_scroll_offset + relative_index;
        let vis = self.visible_tree_indices();
        if actual < vis.len() {
            self.tree_index = actual;
        }
    }

    pub(super) fn explorer_toggle_select(&mut self) {
        if let Some(&real_idx) = self.visible_tree_indices().get(self.tree_index)
            && let Some(node) = self.tree_nodes.get_mut(real_idx)
            && node.is_dir
        {
            node.expanded = !node.expanded;
        }
        self.refresh_status();
    }

    pub(super) fn explorer_status_message(&self) -> String {
        format!(
            "Workspace Explorer: {} item(s). Press [Enter/Space] expand/collapse, [Left/Right] navigate.",
            self.visible_tree_indices().len()
        )
    }
}
