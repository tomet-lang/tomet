//! State management for the Tomet Workbench TUI.
//!
//! `App` aggregates state for all 4 tabs (Explorer/Migration/BatchMeta/
//! StructuralGrep). Per-tab behavior lives in this module's submodules
//! (`explorer`/`migration`/`batch_meta`/`structural`/`inline_editor`);
//! this file keeps only the struct/enum definitions, construction, and
//! thin cross-tab dispatchers (`move_up`/`toggle_select`/`execute_action`/
//! etc.) that route to a `pub(super)` per-tab method based on
//! `active_tab`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::engine::migration::MigrationItem;
use tomet_edit::batch_meta::{BatchMetaEngine, MetaFileEntry};
use tomet_edit::structural::StructuralMatch;
use tomet_indexer::workspace_scan::{FileTreeNode, WorkspaceIndex};

mod batch_meta;
mod explorer;
mod inline_editor;
mod migration;
mod structural;

pub use inline_editor::InlineEditor;

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
    tree_expanded: HashMap<PathBuf, bool>,

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
    pub printer_config: tomet_config::PrinterConfig,
    pub config_root: PathBuf,

    // Workspace catalog: `index` holds filesystem-derived facts only
    // (which paths exist, what kind they are); `tree_nodes`/
    // `migration_items`/`meta_entries` are views derived from it and
    // reconciled (not replaced) against it in `sync_workspace_views` so
    // that per-file UI state (selected/converted/expanded/loaded preview
    // content) survives a reindex of unrelated files.
    index: WorkspaceIndex,
}

impl App {
    pub fn new(dir_path: PathBuf, config_path: Option<PathBuf>) -> Self {
        let (resolved_config_path, printer_config, config_root) = if let Some(ref path) =
            config_path
        {
            let cfg = tomet_config::load_config_from_file(path).unwrap_or_default();
            (Some(path.clone()), cfg, dir_path.clone())
        } else if let Some((cfg, found_path, root)) = tomet_config::find_config_file(&dir_path)
        {
            (Some(found_path), cfg, root)
        } else {
            (
                None,
                tomet_config::PrinterConfig::default(),
                dir_path.clone(),
            )
        };

        let index = WorkspaceIndex::build(&dir_path, &printer_config, &config_root);
        let structural_matches = Vec::new();

        let mut app = Self {
            dir_path,
            active_tab: ActiveTab::Explorer,
            status_message: "Welcome to Tomet Workbench! Press Tab/1/2/3/4 to switch views."
                .to_string(),
            should_quit: false,
            focused_pane: FocusedPane::List,

            tree_nodes: Vec::new(),
            tree_index: 0,
            tree_filter_input: String::new(),
            is_filtering_tree: false,
            tree_expanded: HashMap::new(),

            migration_items: Vec::new(),
            migration_index: 0,

            meta_entries: Vec::new(),
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
            index,
        };

        app.sync_workspace_views();
        app.refresh_status();
        app
    }

    /// Re-derives `tree_nodes`/`migration_items`/`meta_entries` from
    /// `self.index`'s current catalog, reconciling by path against the
    /// existing values rather than replacing them outright -- so
    /// `selected`/`converted`/loaded preview content survives for every
    /// file the triggering action didn't touch. Call this after any
    /// `self.index` mutation (`build`/`rebuild`/`refresh_path`).
    fn sync_workspace_views(&mut self) {
        let mut tree_nodes = self.index.tree_nodes();
        for node in &mut tree_nodes {
            match self.tree_expanded.get(&node.path) {
                Some(&expanded) => node.expanded = expanded,
                None => {
                    self.tree_expanded.insert(node.path.clone(), node.expanded);
                }
            }
        }
        self.tree_nodes = tree_nodes;
        let live: HashSet<PathBuf> = self.tree_nodes.iter().map(|n| n.path.clone()).collect();
        self.tree_expanded.retain(|p, _| live.contains(p));

        let mut old_migration: HashMap<PathBuf, MigrationItem> = self
            .migration_items
            .drain(..)
            .map(|i| (i.source_path.clone(), i))
            .collect();
        self.migration_items = self
            .index
            .migration_candidates()
            .into_iter()
            .map(|c| {
                old_migration
                    .remove(&c.source_path)
                    .unwrap_or_else(|| MigrationItem::from_candidate(c))
            })
            .collect();

        let mut old_meta: HashMap<PathBuf, MetaFileEntry> = self
            .meta_entries
            .drain(..)
            .map(|e| (e.path.clone(), e))
            .collect();
        self.meta_entries = self
            .index
            .meta_paths()
            .into_iter()
            .map(|p| {
                old_meta
                    .remove(&p)
                    .unwrap_or_else(|| BatchMetaEngine::entries_from_paths(vec![p]).remove(0))
            })
            .collect();

        let tv = self.visible_tree_indices().len();
        if self.tree_index >= tv {
            self.tree_index = tv.saturating_sub(1);
        }
        let mv = self.visible_migration_tree_indices().len();
        if self.migration_index >= mv {
            self.migration_index = mv.saturating_sub(1);
        }
        if self.meta_index >= self.meta_entries.len() {
            self.meta_index = self.meta_entries.len().saturating_sub(1);
        }
    }

    pub fn refresh_status(&mut self) {
        self.status_message = match self.active_tab {
            ActiveTab::Explorer => self.explorer_status_message(),
            ActiveTab::Migration => self.migration_status_message(),
            ActiveTab::BatchMeta => self.batch_meta_status_message(),
            ActiveTab::StructuralGrep => self.structural_status_message(),
        };
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
            ActiveTab::Explorer => self.explorer_move_up(),
            ActiveTab::Migration => self.migration_move_up(),
            ActiveTab::BatchMeta => self.batch_meta_move_up(),
            ActiveTab::StructuralGrep => self.structural_move_up(),
        }
    }

    pub fn move_down(&mut self) {
        self.preview_scroll = 0;
        match self.active_tab {
            ActiveTab::Explorer => self.explorer_move_down(),
            ActiveTab::Migration => self.migration_move_down(),
            ActiveTab::BatchMeta => self.batch_meta_move_down(),
            ActiveTab::StructuralGrep => self.structural_move_down(),
        }
    }

    pub fn select_item(&mut self, relative_index: usize) {
        self.preview_scroll = 0;
        match self.active_tab {
            ActiveTab::Explorer => self.explorer_select_item(relative_index),
            ActiveTab::Migration => self.migration_select_item(relative_index),
            ActiveTab::BatchMeta => self.batch_meta_select_item(relative_index),
            ActiveTab::StructuralGrep => self.structural_select_item(relative_index),
        }
    }

    pub fn scroll_preview_down(&mut self, amount: u16) {
        self.preview_scroll = self.preview_scroll.saturating_add(amount);
    }

    pub fn scroll_preview_up(&mut self, amount: u16) {
        self.preview_scroll = self.preview_scroll.saturating_sub(amount);
    }

    pub fn toggle_select(&mut self) {
        match self.active_tab {
            ActiveTab::Explorer => self.explorer_toggle_select(),
            ActiveTab::Migration => self.migration_toggle_select(),
            ActiveTab::BatchMeta => self.batch_meta_toggle_select(),
            ActiveTab::StructuralGrep => self.structural_toggle_select(),
        }
    }

    pub fn toggle_select_all(&mut self) {
        match self.active_tab {
            ActiveTab::Explorer => {}
            ActiveTab::Migration => self.migration_toggle_select_all(),
            ActiveTab::BatchMeta => self.batch_meta_toggle_select_all(),
            ActiveTab::StructuralGrep => self.structural_toggle_select_all(),
        }
    }

    pub fn request_execute_action(&mut self) {
        match self.active_tab {
            ActiveTab::Explorer => {}
            ActiveTab::Migration => self.migration_request_action(),
            ActiveTab::BatchMeta => self.batch_meta_request_action(),
            ActiveTab::StructuralGrep => self.structural_request_action(),
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
            ActiveTab::Migration => self.migration_execute_action(),
            ActiveTab::BatchMeta => self.batch_meta_execute_action(),
            ActiveTab::StructuralGrep => self.structural_execute_action(),
        }
    }

    pub fn reload_workspace(&mut self) {
        self.index.rebuild();
        self.sync_workspace_views();
        self.refresh_status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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

        let config_file = config_dir.join("default.config.tmt");
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

    #[test]
    fn migration_execute_action_preserves_untouched_item_state() {
        let temp_dir =
            std::env::temp_dir().join(format!("tm_test_tui_reconcile_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join("a.md"), "# A").unwrap();
        std::fs::write(temp_dir.join("b.md"), "# B").unwrap();

        let mut app = App::new(temp_dir.clone(), None);
        assert_eq!(app.migration_items.len(), 2);

        // Simulate the user having previewed "b.md" (lazy-loads its
        // content) but not selecting it for conversion, while selecting
        // "a.md" for conversion.
        for item in &mut app.migration_items {
            if item.source_path.file_name().unwrap() == "b.md" {
                item.ensure_loaded();
                assert!(!item.markdown_src.is_empty());
            } else {
                item.selected = true;
            }
        }

        app.migration_execute_action();

        assert_eq!(
            app.migration_items.len(),
            2,
            "unrelated files must not be dropped from the list"
        );
        let a = app
            .migration_items
            .iter()
            .find(|i| i.source_path.file_name().unwrap() == "a.md")
            .unwrap();
        assert!(a.converted, "a.md should have been converted");

        let b = app
            .migration_items
            .iter()
            .find(|i| i.source_path.file_name().unwrap() == "b.md")
            .unwrap();
        assert!(
            !b.converted && !b.selected,
            "b.md was never selected for conversion, its state must be untouched"
        );
        assert!(
            !b.markdown_src.is_empty(),
            "b.md's already-loaded preview content must survive the reload \
             triggered by converting an unrelated file"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    trait ContainsStdPath {
        fn contains_std_path(&self, other: &Path) -> bool;
    }

    impl ContainsStdPath for PathBuf {
        fn contains_std_path(&self, other: &Path) -> bool {
            self.starts_with(other)
        }
    }
}
