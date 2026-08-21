//! State management for the TypedMark Workbench TUI.

use std::path::{Path, PathBuf};

use super::engine::batch_meta::{BatchMetaEngine, MetaFileEntry};
use super::engine::migration::{FileTreeNode, MigrationEngine, MigrationItem};
use super::engine::structural::{
    StructuralAction, StructuralEngine, StructuralMatch, StructuralQuery,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Explorer = 0,
    Migration = 1,
    BatchMeta = 2,
    StructuralGrep = 3,
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

#[derive(Debug, Clone)]
pub struct PendingConfirm {
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedPane {
    List,
    Preview,
    Controls,
}

pub struct App {
    pub dir_path: PathBuf,
    pub active_tab: ActiveTab,
    pub status_message: String,
    pub should_quit: bool,
    pub focused_pane: FocusedPane,

    // Tab 1: Workspace Explorer State
    pub tree_nodes: Vec<FileTreeNode>,
    pub tree_index: usize,
    pub tree_filter_input: String,
    pub is_filtering_tree: bool,

    // Tab 2: Migration State
    pub migration_items: Vec<MigrationItem>,
    pub migration_index: usize,

    // Tab 3: Batch Meta State
    pub meta_entries: Vec<MetaFileEntry>,
    pub meta_index: usize,
    pub meta_key_input: String,
    pub meta_val_input: String,
    pub meta_edit_target: Option<MetaEditTarget>,

    // Tab 4: Structural Grep State
    pub structural_matches: Vec<StructuralMatch>,
    pub structural_index: usize,
    pub query_tag_input: String,
    pub query_key_input: String,
    pub query_val_input: String,
    pub replace_to_input: String,
    pub structural_input_target: Option<StructuralInputTarget>,

    // UI Scroll State
    pub preview_scroll: u16,
    pub tree_scroll_offset: usize,
    pub migration_scroll_offset: usize,
    pub meta_scroll_offset: usize,
    pub structural_scroll_offset: usize,

    // Inline File Editor State
    pub inline_editor: Option<InlineEditor>,

    // Confirmation Modal State
    pub pending_confirm: Option<PendingConfirm>,

    // Printer Formatting Config
    pub printer_config_path: Option<PathBuf>,
    pub printer_config: typedmark_printer::PrinterConfig,
    pub config_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct InlineEditor {
    pub file_path: PathBuf,
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub is_dirty: bool,
}

impl InlineEditor {
    pub fn insert_char(&mut self, c: char) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.cursor_row >= self.lines.len() {
            self.cursor_row = self.lines.len() - 1;
        }
        let line = &mut self.lines[self.cursor_row];
        if self.cursor_col > line.chars().count() {
            self.cursor_col = line.chars().count();
        }
        let byte_pos = line
            .char_indices()
            .nth(self.cursor_col)
            .map(|(idx, _)| idx)
            .unwrap_or_else(|| line.len());
        line.insert(byte_pos, c);
        self.cursor_col += 1;
        self.is_dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        if self.cursor_row >= self.lines.len() {
            self.cursor_row = self.lines.len() - 1;
        }
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let byte_pos = line
                .char_indices()
                .nth(self.cursor_col - 1)
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            line.remove(byte_pos);
            self.cursor_col -= 1;
            self.is_dirty = true;
        } else if self.cursor_row > 0 {
            let current_line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            let prev_line = &mut self.lines[self.cursor_row];
            self.cursor_col = prev_line.chars().count();
            prev_line.push_str(&current_line);
            self.is_dirty = true;
        }
    }

    pub fn delete(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        if self.cursor_row >= self.lines.len() {
            return;
        }
        let line_len = self.lines[self.cursor_row].chars().count();
        if self.cursor_col < line_len {
            let line = &mut self.lines[self.cursor_row];
            let byte_pos = line
                .char_indices()
                .nth(self.cursor_col)
                .map(|(idx, _)| idx)
                .unwrap_or(line.len());
            line.remove(byte_pos);
            self.is_dirty = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            let next_line = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next_line);
            self.is_dirty = true;
        }
    }

    pub fn insert_newline(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.cursor_row >= self.lines.len() {
            self.cursor_row = self.lines.len() - 1;
        }
        let line = &mut self.lines[self.cursor_row];
        let byte_pos = line
            .char_indices()
            .nth(self.cursor_col)
            .map(|(idx, _)| idx)
            .unwrap_or_else(|| line.len());
        let remainder = line.split_off(byte_pos);
        self.lines.insert(self.cursor_row + 1, remainder);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.is_dirty = true;
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            let len = self.lines[self.cursor_row].chars().count();
            if self.cursor_col > len {
                self.cursor_col = len;
            }
        }
    }

    pub fn move_down(&mut self) {
        if !self.lines.is_empty() && self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            let len = self.lines[self.cursor_row].chars().count();
            if self.cursor_col > len {
                self.cursor_col = len;
            }
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].chars().count();
        }
    }

    pub fn move_right(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        let line_len = self.lines[self.cursor_row].chars().count();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    pub fn move_end(&mut self) {
        if !self.lines.is_empty() && self.cursor_row < self.lines.len() {
            self.cursor_col = self.lines[self.cursor_row].chars().count();
        }
    }
}

impl App {
    pub fn new(dir_path: PathBuf, config_path: Option<PathBuf>) -> Self {
        let (resolved_config_path, printer_config, config_root) =
            if let Some(ref path) = config_path {
                let cfg = typedmark_printer::load_config_from_file(path).unwrap_or_default();
                (Some(path.clone()), cfg, dir_path.clone())
            } else if let Some((cfg, found_path, root)) =
                typedmark_printer::find_config_file(&dir_path)
            {
                (Some(found_path), cfg, root)
            } else {
                (
                    None,
                    typedmark_printer::PrinterConfig::default(),
                    dir_path.clone(),
                )
            };

        let tree_nodes =
            MigrationEngine::scan_tree_with_config(&dir_path, &printer_config, &config_root);
        let migration_items =
            MigrationEngine::scan_with_config(&dir_path, &printer_config, &config_root);
        let meta_entries =
            BatchMetaEngine::scan_with_config(&dir_path, &printer_config, &config_root);
        let structural_matches = Vec::new();

        let mut app = Self {
            dir_path,
            active_tab: ActiveTab::Explorer,
            status_message: "Welcome to TypedMark Workbench! Press Tab/1/2/3/4 to switch views."
                .to_string(),
            should_quit: false,
            focused_pane: FocusedPane::List,

            tree_nodes,
            tree_index: 0,
            tree_filter_input: String::new(),
            is_filtering_tree: false,

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

            preview_scroll: 0,
            tree_scroll_offset: 0,
            migration_scroll_offset: 0,
            meta_scroll_offset: 0,
            structural_scroll_offset: 0,

            inline_editor: None,
            pending_confirm: None,
            printer_config_path: resolved_config_path,
            printer_config,
            config_root,
        };

        app.refresh_status();
        app
    }

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

    pub fn refresh_status(&mut self) {
        match self.active_tab {
            ActiveTab::Explorer => {
                self.status_message = format!(
                    "Workspace Explorer: {} item(s). Press [Enter/Space] expand/collapse, [Left/Right] navigate.",
                    self.visible_tree_indices().len()
                );
            }
            ActiveTab::Migration => {
                let selected_count = self
                    .migration_items
                    .iter()
                    .filter(|i| i.selected && !i.converted)
                    .count();
                self.status_message = format!(
                    "Found {} Markdown file(s) ({} selected). Press [Space] to toggle, [a] toggle all, [e] to convert.",
                    self.migration_items.len(),
                    selected_count
                );
            }
            ActiveTab::BatchMeta => {
                let selected_count = self.meta_entries.iter().filter(|e| e.selected).count();
                self.status_message = format!(
                    "Loaded {} .tm file(s) ({} selected). Press [k]ey/[v]al to set metadata, [e] to apply.",
                    self.meta_entries.len(),
                    selected_count
                );
            }
            ActiveTab::StructuralGrep => {
                let selected_count = self
                    .structural_matches
                    .iter()
                    .filter(|m| m.selected)
                    .count();
                self.status_message = format!(
                    "Matched {} file(s) ({} selected). Press [t]/[k]/[v]/[r] field, [s]earch, [e] to refactor.",
                    self.structural_matches.len(),
                    selected_count
                );
            }
        }
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

    pub fn adjust_preview_scrolloff(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        let scrolloff = 2.min(height / 2);

        let (target_row, total_lines) = if let Some(editor) = &self.inline_editor {
            (editor.cursor_row, editor.lines.len())
        } else {
            return;
        };

        if total_lines == 0 {
            return;
        }
        let mut offset = self.preview_scroll as usize;

        if target_row < offset + scrolloff {
            offset = target_row.saturating_sub(scrolloff);
        } else if target_row + scrolloff >= offset + height {
            offset = (target_row + scrolloff + 1).saturating_sub(height);
        }

        let max_offset = total_lines.saturating_sub(height);
        if offset > max_offset {
            offset = max_offset;
        }

        self.preview_scroll = offset as u16;
    }

    pub fn next_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Explorer => ActiveTab::Migration,
            ActiveTab::Migration => ActiveTab::BatchMeta,
            ActiveTab::BatchMeta => ActiveTab::StructuralGrep,
            ActiveTab::StructuralGrep => ActiveTab::Explorer,
        };
        self.preview_scroll = 0;
        self.refresh_status();
    }

    pub fn previous_tab(&mut self) {
        self.active_tab = match self.active_tab {
            ActiveTab::Explorer => ActiveTab::StructuralGrep,
            ActiveTab::Migration => ActiveTab::Explorer,
            ActiveTab::BatchMeta => ActiveTab::Migration,
            ActiveTab::StructuralGrep => ActiveTab::BatchMeta,
        };
        self.preview_scroll = 0;
        self.refresh_status();
    }

    pub fn move_up(&mut self) {
        self.preview_scroll = 0;
        match self.active_tab {
            ActiveTab::Explorer => {
                if self.tree_index > 0 {
                    self.tree_index -= 1;
                }
            }
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
        self.preview_scroll = 0;
        match self.active_tab {
            ActiveTab::Explorer => {
                let vis_count = self.visible_tree_indices().len();
                if vis_count > 0 && self.tree_index < vis_count - 1 {
                    self.tree_index += 1;
                }
            }
            ActiveTab::Migration => {
                let vis_count = self.visible_migration_tree_indices().len();
                if vis_count > 0 && self.migration_index < vis_count - 1 {
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

    pub fn select_item(&mut self, relative_index: usize) {
        self.preview_scroll = 0;
        match self.active_tab {
            ActiveTab::Explorer => {
                let actual = self.tree_scroll_offset + relative_index;
                let vis = self.visible_tree_indices();
                if actual < vis.len() {
                    self.tree_index = actual;
                }
            }
            ActiveTab::Migration => {
                let actual = self.migration_scroll_offset + relative_index;
                let vis = self.visible_migration_tree_indices();
                if actual < vis.len() {
                    self.migration_index = actual;
                }
            }
            ActiveTab::BatchMeta => {
                let actual = self.meta_scroll_offset + relative_index;
                if actual < self.meta_entries.len() {
                    self.meta_index = actual;
                }
            }
            ActiveTab::StructuralGrep => {
                let actual = self.structural_scroll_offset + relative_index;
                if actual < self.structural_matches.len() {
                    self.structural_index = actual;
                }
            }
        }
    }

    pub fn scroll_preview_down(&mut self, amount: u16) {
        self.preview_scroll = self.preview_scroll.saturating_add(amount);
    }

    pub fn scroll_preview_up(&mut self, amount: u16) {
        self.preview_scroll = self.preview_scroll.saturating_sub(amount);
    }

    pub fn clear_structural_inputs(&mut self) {
        self.query_tag_input.clear();
        self.query_key_input.clear();
        self.query_val_input.clear();
        self.replace_to_input.clear();
        self.structural_input_target = None;
        self.run_structural_query();
    }

    pub fn toggle_select(&mut self) {
        match self.active_tab {
            ActiveTab::Explorer => {
                if let Some(&real_idx) = self.visible_tree_indices().get(self.tree_index) {
                    if let Some(node) = self.tree_nodes.get_mut(real_idx) {
                        if node.is_dir {
                            node.expanded = !node.expanded;
                        }
                    }
                }
                self.refresh_status();
            }
            ActiveTab::Migration => {
                let vis = self.visible_migration_tree_indices();
                if let Some(&real_idx) = vis.get(self.migration_index) {
                    if self.tree_nodes[real_idx].is_dir {
                        self.tree_nodes[real_idx].expanded = !self.tree_nodes[real_idx].expanded;
                    }
                    self.toggle_migration_node(real_idx);
                }
                self.refresh_status();
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
            ActiveTab::Explorer => {}
            ActiveTab::Migration => {
                let all_selected = self.migration_items.iter().all(|i| i.selected);
                for i in &mut self.migration_items {
                    i.selected = !all_selected;
                }
                self.refresh_status();
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

    pub fn request_execute_action(&mut self) {
        match self.active_tab {
            ActiveTab::Explorer => {}
            ActiveTab::Migration => {
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
            ActiveTab::BatchMeta => {
                let count = self.meta_entries.iter().filter(|e| e.selected).count();
                if count == 0 {
                    self.status_message =
                        "No .tm files selected! Press [Space] to select or [a] to select all."
                            .to_string();
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
            ActiveTab::StructuralGrep => {
                let count = self
                    .structural_matches
                    .iter()
                    .filter(|m| m.selected)
                    .count();
                if count == 0 {
                    self.status_message =
                        "No matches selected! Press [Space] to select or [a] to select all."
                            .to_string();
                } else if self.replace_to_input.is_empty() {
                    self.status_message = "Please specify a REPLACE target value.".to_string();
                } else {
                    self.pending_confirm = Some(PendingConfirm {
                        message: format!("Apply refactoring to {count} file(s)?"),
                    });
                }
            }
        }
    }

    pub fn confirm_action(&mut self) {
        if self.pending_confirm.take().is_some() {
            self.execute_action();
        }
    }

    pub fn cancel_confirmation(&mut self) {
        self.pending_confirm = None;
        self.status_message = "Action cancelled.".to_string();
    }

    pub fn execute_action(&mut self) {
        match self.active_tab {
            ActiveTab::Explorer => {}
            ActiveTab::Migration => {
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
                    let action = if !self.query_val_input.is_empty() {
                        StructuralAction::ReplaceValue {
                            key: self.query_key_input.clone(),
                            new_value: self.replace_to_input.clone(),
                        }
                    } else {
                        StructuralAction::RenameKey {
                            old_key: self.query_key_input.clone(),
                            new_key: self.replace_to_input.clone(),
                        }
                    };
                    StructuralEngine::apply_action(&mut self.structural_matches, &action);
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
        self.structural_matches = StructuralEngine::search_with_config(
            &self.dir_path,
            &query,
            &self.printer_config,
            &self.config_root,
        );
        self.structural_index = 0;
        self.status_message = format!("Found {} matching file(s).", self.structural_matches.len());
    }

    pub fn start_inline_editor(&mut self) {
        if self.active_tab != ActiveTab::Explorer {
            return;
        }
        let vis = self.visible_tree_indices();
        if let Some(&real_idx) = vis.get(self.tree_index) {
            if let Some(node) = self.tree_nodes.get(real_idx) {
                if !node.is_dir {
                    if let Ok(src) = std::fs::read_to_string(&node.path) {
                        let lines: Vec<String> = src.lines().map(|s| s.to_string()).collect();
                        let lines = if lines.is_empty() {
                            vec![String::new()]
                        } else {
                            lines
                        };
                        self.inline_editor = Some(InlineEditor {
                            file_path: node.path.clone(),
                            lines,
                            cursor_row: 0,
                            cursor_col: 0,
                            is_dirty: false,
                        });
                        self.focused_pane = FocusedPane::Preview;
                        self.status_message = format!(
                            "EDITING {}: Press [Ctrl+S] Save, [Esc] Exit edit mode.",
                            node.name
                        );
                    }
                }
            }
        }
    }

    pub fn save_inline_editor(&mut self) {
        if let Some(editor) = &mut self.inline_editor {
            let content = editor.lines.join("\n");
            if std::fs::write(&editor.file_path, content).is_ok() {
                editor.is_dirty = false;
                let path_display = editor.file_path.display().to_string();
                self.status_message = format!("Saved {path_display} successfully!");
                self.tree_nodes = MigrationEngine::scan_tree_with_config(
                    &self.dir_path,
                    &self.printer_config,
                    &self.config_root,
                );
                self.migration_items = MigrationEngine::scan_with_config(
                    &self.dir_path,
                    &self.printer_config,
                    &self.config_root,
                );
                self.meta_entries = BatchMetaEngine::scan_with_config(
                    &self.dir_path,
                    &self.printer_config,
                    &self.config_root,
                );
            } else {
                self.status_message = format!("Failed to write to {}", editor.file_path.display());
            }
        }
    }

    pub fn stop_inline_editor(&mut self, force_discard: bool) {
        if !force_discard {
            if let Some(editor) = &self.inline_editor {
                if editor.is_dirty {
                    self.pending_confirm = Some(PendingConfirm {
                        message: "Unsaved changes! Discard changes and exit edit mode?".to_string(),
                    });
                    return;
                }
            }
        }
        self.inline_editor = None;
        self.refresh_status();
    }

    pub fn get_selected_file_path(&self) -> Option<PathBuf> {
        if self.active_tab != ActiveTab::Explorer {
            return None;
        }
        let vis = self.visible_tree_indices();
        if let Some(&real_idx) = vis.get(self.tree_index) {
            if let Some(node) = self.tree_nodes.get(real_idx) {
                if !node.is_dir {
                    return Some(node.path.clone());
                }
            }
        }
        None
    }

    pub fn reload_workspace(&mut self) {
        self.tree_nodes = MigrationEngine::scan_tree_with_config(
            &self.dir_path,
            &self.printer_config,
            &self.config_root,
        );
        self.migration_items = MigrationEngine::scan_with_config(
            &self.dir_path,
            &self.printer_config,
            &self.config_root,
        );
        self.meta_entries = BatchMetaEngine::scan_with_config(
            &self.dir_path,
            &self.printer_config,
            &self.config_root,
        );
        self.refresh_status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new_with_explicit_config_ignore_rules() {
        let temp_dir =
            std::env::temp_dir().join(format!("tm_test_tui_ignore_{}", std::process::id()));
        let config_dir = temp_dir.join("config");
        let vault_dir = temp_dir.join("obsidian-main");
        let ignored_dir = vault_dir.join("00-09 System/01 Apps/obsidian");
        let normal_dir = vault_dir.join("10 Notes");

        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&ignored_dir).unwrap();
        std::fs::create_dir_all(&normal_dir).unwrap();

        let config_file = config_dir.join("default.config.tm");
        std::fs::write(
            &config_file,
            r#"@settings(format:json){
  {
    "ignore": {
      "files": [
        "00-09 System/01 Apps/obsidian"
      ]
    }
  }
}
"#,
        )
        .unwrap();

        std::fs::write(ignored_dir.join("ignored_note.md"), "# Ignored").unwrap();
        std::fs::write(normal_dir.join("kept_note.md"), "# Kept").unwrap();

        let app = App::new(vault_dir.clone(), Some(config_file));

        let migration_paths: Vec<_> = app
            .migration_items
            .iter()
            .map(|item| item.source_path.clone())
            .collect();
        assert!(
            migration_paths
                .iter()
                .all(|p| !p.contains_std_path(&ignored_dir)),
            "ignored_dir files should not be in migration_items: {migration_paths:?}"
        );
        assert_eq!(app.migration_items.len(), 1);
        assert_eq!(
            app.migration_items[0].source_path,
            normal_dir.join("kept_note.md")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

#[cfg(test)]
trait ContainsStdPath {
    fn contains_std_path(&self, other: &Path) -> bool;
}

#[cfg(test)]
impl ContainsStdPath for PathBuf {
    fn contains_std_path(&self, other: &Path) -> bool {
        self.starts_with(other)
    }
}
