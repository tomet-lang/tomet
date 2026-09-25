//! Inline file editor state (Explorer tab's `[i]` edit mode) and its
//! own cursor/text-editing operations, plus `App`'s
//! start/save/stop-editing entry points and preview-pane scrolloff.

use std::path::PathBuf;

use super::{App, FocusedPane, PendingConfirm};

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

    pub fn start_inline_editor(&mut self) {
        if self.active_tab != super::ActiveTab::Explorer {
            return;
        }
        let vis = self.visible_tree_indices();
        if let Some(&real_idx) = vis.get(self.tree_index)
            && let Some(node) = self.tree_nodes.get(real_idx)
            && !node.is_dir
            && let Ok(src) = std::fs::read_to_string(&node.path)
        {
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

    pub fn save_inline_editor(&mut self) {
        if let Some(editor) = &mut self.inline_editor {
            let content = editor.lines.join("\n");
            if std::fs::write(&editor.file_path, content).is_ok() {
                editor.is_dirty = false;
                let path_display = editor.file_path.display().to_string();
                self.status_message = format!("Saved {path_display} successfully!");
                let saved_path = editor.file_path.clone();
                // The saved path already existed and isn't renamed by this
                // write, so its catalog entry can't change -- no need to
                // touch `self.index`/`sync_workspace_views`. Only the
                // lazily-loaded preview caches for *this* path need
                // invalidating, since they'd otherwise keep serving
                // pre-edit content.
                if let Some(item) = self
                    .migration_items
                    .iter_mut()
                    .find(|i| i.source_path == saved_path)
                {
                    item.markdown_src.clear();
                    item.tomet_src.clear();
                }
                if let Some(entry) = self.meta_entries.iter_mut().find(|e| e.path == saved_path) {
                    entry.original_src.clear();
                    entry.modified_src.clear();
                    entry.metadata.clear();
                }
            } else {
                self.status_message = format!("Failed to write to {}", editor.file_path.display());
            }
        }
    }

    pub fn stop_inline_editor(&mut self, force_discard: bool) {
        if !force_discard
            && let Some(editor) = &self.inline_editor
            && editor.is_dirty
        {
            self.pending_confirm = Some(PendingConfirm {
                message: "Unsaved changes! Discard changes and exit edit mode?".to_string(),
            });
            return;
        }
        self.inline_editor = None;
        self.refresh_status();
    }
}
