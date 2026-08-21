//! Markdown -> TypedMark Migration Engine.

use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};

use typedmark_config::PrinterConfig;
use typedmark_printer::document_to_tm_with_config;

#[derive(Debug, Clone)]
pub struct MigrationItem {
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub markdown_src: String,
    pub typedmark_src: String,
    pub selected: bool,
    pub converted: bool,
}

#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
    pub migration_item: Option<MigrationItem>,
    pub has_md_children: bool,
}

impl MigrationItem {
    pub fn ensure_loaded(&mut self) {
        self.ensure_loaded_with_config(&PrinterConfig::default());
    }

    pub fn ensure_loaded_with_config(&mut self, config: &PrinterConfig) {
        if self.markdown_src.is_empty() {
            if let Ok(src) = fs::read_to_string(&self.source_path) {
                let mut doc = typedmark_markdown::from_markdown(&src);
                typedmark_printer::ensure_document_id_with_config(&mut doc, config);
                self.typedmark_src = document_to_tm_with_config(&doc, config);
                self.markdown_src = src;
            }
        }
    }
}

pub struct MigrationEngine;

impl MigrationEngine {
    /// Scan workspace directory tree for relevant files (.md/.tm), compacting single-child directory chains.
    pub fn scan_tree(root: &Path) -> Vec<FileTreeNode> {
        let (config, _, config_root) =
            typedmark_config::find_config_file(root).unwrap_or_else(|| {
                (
                    PrinterConfig::default(),
                    root.to_path_buf(),
                    root.to_path_buf(),
                )
            });
        Self::scan_tree_with_config(root, &config, &config_root)
    }

    pub fn scan_tree_with_config(
        root: &Path,
        config: &PrinterConfig,
        config_root: &Path,
    ) -> Vec<FileTreeNode> {
        let mut raw_tree = build_raw_tree_with_config(root, config, config_root);
        compact_raw_nodes(&mut raw_tree);
        let mut nodes = Vec::new();
        flatten_raw_nodes(&raw_tree, 0, &mut nodes);
        nodes
    }
    /// Scan a directory or single file path for Markdown files (`.md`), respecting `.gitignore`.
    #[allow(dead_code)]
    pub fn scan(path: &Path) -> Vec<MigrationItem> {
        let (config, _, config_root) =
            typedmark_config::find_config_file(path).unwrap_or_else(|| {
                (
                    PrinterConfig::default(),
                    path.to_path_buf(),
                    path.to_path_buf(),
                )
            });
        Self::scan_with_config(path, &config, &config_root)
    }

    pub fn scan_with_config(
        path: &Path,
        config: &PrinterConfig,
        config_root: &Path,
    ) -> Vec<MigrationItem> {
        let mut items = Vec::new();
        if path.is_file() {
            if is_markdown_file(path) {
                if let Some(item) = Self::load_item(path) {
                    items.push(item);
                }
            }
        } else if path.is_dir() {
            for entry in WalkBuilder::new(path)
                .hidden(true)
                .git_ignore(true)
                .build()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
            {
                let p = entry.path();
                if typedmark_indexer::is_path_ignored(p, Some(config_root), &config.ignore_files) {
                    continue;
                }
                if is_markdown_file(p) {
                    if let Some(item) = Self::load_item(p) {
                        items.push(item);
                    }
                }
            }
        }
        items.sort_by(|a, b| a.source_path.cmp(&b.source_path));
        items
    }

    fn load_item(path: &Path) -> Option<MigrationItem> {
        let target_path = path.with_extension("tm");
        Some(MigrationItem {
            source_path: path.to_path_buf(),
            target_path,
            markdown_src: String::new(),
            typedmark_src: String::new(),
            selected: false,
            converted: false,
        })
    }

    /// Convert selected items and write `.tm` files using specified printer config.
    pub fn execute_with_config(
        items: &mut [MigrationItem],
        remove_original: bool,
        config: &PrinterConfig,
    ) -> anyhow::Result<usize> {
        let mut count = 0;
        for item in items.iter_mut() {
            if !item.selected || item.converted {
                continue;
            }
            item.ensure_loaded_with_config(config);
            fs::write(&item.target_path, &item.typedmark_src)?;
            item.converted = true;
            count += 1;
            if remove_original && item.source_path != item.target_path {
                let _ = fs::remove_file(&item.source_path);
            }
        }
        Ok(count)
    }

    /// Convert selected items and write `.tm` files with default config.
    pub fn execute(items: &mut [MigrationItem], remove_original: bool) -> anyhow::Result<usize> {
        Self::execute_with_config(items, remove_original, &PrinterConfig::default())
    }
}

fn is_markdown_file(p: &Path) -> bool {
    p.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}

fn is_relevant_file(p: &Path) -> bool {
    p.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            ext.eq_ignore_ascii_case("md")
                || ext.eq_ignore_ascii_case("markdown")
                || ext.eq_ignore_ascii_case("tm")
                || ext.eq_ignore_ascii_case("tmt")
        })
        .unwrap_or(false)
}

struct RawNode {
    path: PathBuf,
    name: String,
    is_dir: bool,
    children: Vec<RawNode>,
    has_md: bool,
}

fn build_raw_tree_with_config(
    root: &Path,
    config: &PrinterConfig,
    config_root: &Path,
) -> Vec<RawNode> {
    use std::collections::BTreeMap;

    let mut children_map: BTreeMap<PathBuf, Vec<RawNode>> = BTreeMap::new();

    let walker = WalkBuilder::new(root).hidden(true).git_ignore(true).build();

    for entry in walker.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p == root {
            continue;
        }
        if typedmark_indexer::is_path_ignored(p, Some(config_root), &config.ignore_files) {
            continue;
        }
        let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());
        let is_file = entry.file_type().map_or(false, |ft| ft.is_file());
        let is_rel_file = is_file && is_relevant_file(p);

        if is_rel_file || is_dir {
            if let Some(parent) = p.parent() {
                let name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();

                let has_md = !is_dir && is_markdown_file(p);

                children_map
                    .entry(parent.to_path_buf())
                    .or_default()
                    .push(RawNode {
                        path: p.to_path_buf(),
                        name,
                        is_dir,
                        children: Vec::new(),
                        has_md,
                    });
            }
        }
    }

    fn assemble(dir: &Path, map: &mut BTreeMap<PathBuf, Vec<RawNode>>) -> Vec<RawNode> {
        let list = map.remove(dir).unwrap_or_default();
        let mut result = Vec::new();

        for mut node in list {
            if node.is_dir {
                node.children = assemble(&node.path, map);
                node.has_md = node.children.iter().any(|c| c.has_md);
                if !node.children.is_empty() {
                    result.push(node);
                }
            } else {
                result.push(node);
            }
        }

        result.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });
        result
    }

    assemble(root, &mut children_map)
}

fn compact_raw_nodes(nodes: &mut Vec<RawNode>) {
    for node in nodes.iter_mut() {
        if node.is_dir {
            compact_raw_nodes(&mut node.children);
            while node.children.len() == 1 && node.children[0].is_dir {
                let child = node.children.remove(0);
                node.name = format!("{}/{}", node.name, child.name);
                node.path = child.path;
                node.has_md = child.has_md;
                node.children = child.children;
            }
        }
    }
}

fn flatten_raw_nodes(nodes: &[RawNode], depth: usize, acc: &mut Vec<FileTreeNode>) {
    for node in nodes {
        let migration_item = if !node.is_dir && is_markdown_file(&node.path) {
            MigrationEngine::load_item(&node.path)
        } else {
            None
        };

        acc.push(FileTreeNode {
            path: node.path.clone(),
            name: node.name.clone(),
            is_dir: node.is_dir,
            depth,
            expanded: depth == 0,
            migration_item,
            has_md_children: node.has_md,
        });

        if node.is_dir {
            flatten_raw_nodes(&node.children, depth + 1, acc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_migration_conversion() {
        let dir_path = std::env::temp_dir().join(format!("tm_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir_path).unwrap();
        let md_path = dir_path.join("doc.md");
        fs::write(&md_path, "# Migration Test\n\n- item 1\n- item 2\n").unwrap();

        let mut items = MigrationEngine::scan(&dir_path);
        assert_eq!(items.len(), 1);
        items[0].ensure_loaded();
        assert!(items[0].typedmark_src.contains("#[Migration Test]"));

        let mut items_to_exec = items;
        items_to_exec[0].selected = true;
        let count = MigrationEngine::execute(&mut items_to_exec, false).unwrap();
        assert_eq!(count, 1);
        assert!(dir_path.join("doc.tm").exists());
        let _ = fs::remove_dir_all(&dir_path);
    }
}
