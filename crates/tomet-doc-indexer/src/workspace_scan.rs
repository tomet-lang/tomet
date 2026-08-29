//! Stateful, incrementally-updatable workspace index: walks a directory
//! once and keeps a flat, path-keyed catalog of it in memory, from which
//! `tomet-tui`'s Explorer tree, Migration candidate list, and BatchMeta
//! path list are all cheaply derived (no I/O) on demand. `refresh_path`/
//! `remove_path` let a caller that just wrote/deleted one specific file
//! update the catalog without re-walking the whole tree.
//!
//! The catalog is deliberately kept *flat* (`BTreeMap<PathBuf, EntryKind>`)
//! rather than as a nested tree: the nested/compacted tree shape (single-
//! child-directory-chain folding, `has_md_children` bubbling) is pure
//! in-memory computation with no I/O, so it's cheap to rebuild from the
//! flat catalog on every `tree_nodes()` call. That sidesteps having to
//! incrementally patch folded directory chains (e.g. un-collapsing `a/b/c`
//! back into three nodes when a new sibling file appears under `b`), which
//! would be much harder to get right.
//!
//! `MigrationCandidate`/`FileTreeNode` carry only filesystem-derived facts
//! (which paths exist, what kind they are) -- no selected/converted/
//! expanded/loaded-content fields. That UI/session state belongs to
//! `tomet-tui`, keyed by path, reconciled against this index's current
//! path set rather than replaced wholesale on every reindex.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use tomet_config::PrinterConfig;

use crate::is_path_ignored;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Dir,
    Markdown,
    Tomet,
}

#[derive(Debug, Clone)]
pub struct MigrationCandidate {
    pub source_path: PathBuf,
    pub target_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
    pub migration_item: Option<MigrationCandidate>,
    pub has_md_children: bool,
}

pub struct WorkspaceIndex {
    root: PathBuf,
    config: PrinterConfig,
    config_root: PathBuf,
    entries: BTreeMap<PathBuf, EntryKind>,
}

impl WorkspaceIndex {
    /// Walks `root` once and builds the initial catalog.
    pub fn build(root: &Path, config: &PrinterConfig, config_root: &Path) -> Self {
        let mut entries = BTreeMap::new();
        populate_entries(root, config, config_root, &mut entries);
        Self {
            root: root.to_path_buf(),
            config: config.clone(),
            config_root: config_root.to_path_buf(),
            entries,
        }
    }

    /// Re-walks `root` from scratch and replaces the catalog wholesale.
    /// Use this when the caller doesn't know exactly what changed (e.g.
    /// after returning from an external `$EDITOR` session).
    pub fn rebuild(&mut self) {
        let mut entries = BTreeMap::new();
        populate_entries(&self.root, &self.config, &self.config_root, &mut entries);
        self.entries = entries;
    }

    /// Re-checks a single path against the filesystem and updates the
    /// catalog accordingly (insert/update/remove). Does not discover
    /// unrelated changes elsewhere in the tree -- assumes the path's
    /// parent directory chain is already tracked, which holds for any
    /// path the caller itself just wrote to.
    pub fn refresh_path(&mut self, path: &Path) {
        if path == self.root {
            return;
        }
        if is_path_ignored(path, Some(&self.config_root), &self.config.ignore_files) {
            self.entries.remove(path);
            return;
        }
        if path.is_dir() {
            self.entries.insert(path.to_path_buf(), EntryKind::Dir);
        } else if path.is_file() {
            match classify_file(path) {
                Some(kind) => {
                    self.entries.insert(path.to_path_buf(), kind);
                }
                None => {
                    self.entries.remove(path);
                }
            }
        } else {
            self.entries.remove(path);
        }
    }

    /// Removes a path from the catalog (e.g. after a delete).
    pub fn remove_path(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    /// Derives the compacted Explorer/Migration tree from the current
    /// catalog. Pure in-memory computation, no I/O.
    pub fn tree_nodes(&self) -> Vec<FileTreeNode> {
        if self.root.is_file() {
            return Vec::new();
        }
        let mut raw_tree = raw_tree_from_entries(&self.root, &self.entries);
        compact_raw_nodes(&mut raw_tree);
        let mut nodes = Vec::new();
        flatten_raw_nodes(&raw_tree, 0, &mut nodes);
        nodes
    }

    /// Every currently-cataloged Markdown file, as a migration candidate.
    pub fn migration_candidates(&self) -> Vec<MigrationCandidate> {
        self.entries
            .iter()
            .filter(|(_, k)| **k == EntryKind::Markdown)
            .map(|(p, _)| MigrationCandidate {
                source_path: p.clone(),
                target_path: p.with_extension("tmt"),
            })
            .collect()
    }

    /// Every currently-cataloged `.tmt`/`.tmt` file.
    pub fn meta_paths(&self) -> Vec<PathBuf> {
        self.entries
            .iter()
            .filter(|(_, k)| **k == EntryKind::Tomet)
            .map(|(p, _)| p.clone())
            .collect()
    }
}

fn populate_entries(
    root: &Path,
    config: &PrinterConfig,
    config_root: &Path,
    out: &mut BTreeMap<PathBuf, EntryKind>,
) {
    if root.is_file() {
        if let Some(kind) = classify_file(root) {
            out.insert(root.to_path_buf(), kind);
        }
        return;
    }
    if !root.is_dir() {
        return;
    }

    let walker = WalkBuilder::new(root).hidden(true).git_ignore(true).build();
    for entry in walker.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p == root {
            continue;
        }
        if is_path_ignored(p, Some(config_root), &config.ignore_files) {
            continue;
        }
        let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());
        let is_file = entry.file_type().map_or(false, |ft| ft.is_file());
        if is_dir {
            out.insert(p.to_path_buf(), EntryKind::Dir);
        } else if is_file {
            if let Some(kind) = classify_file(p) {
                out.insert(p.to_path_buf(), kind);
            }
        }
    }
}

fn classify_file(p: &Path) -> Option<EntryKind> {
    if is_markdown_file(p) {
        Some(EntryKind::Markdown)
    } else if crate::is_tm_file(p) {
        Some(EntryKind::Tomet)
    } else {
        None
    }
}

fn is_markdown_file(p: &Path) -> bool {
    p.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}

struct RawNode {
    path: PathBuf,
    name: String,
    is_dir: bool,
    children: Vec<RawNode>,
    has_md: bool,
}

fn raw_tree_from_entries(root: &Path, entries: &BTreeMap<PathBuf, EntryKind>) -> Vec<RawNode> {
    let mut children_map: BTreeMap<PathBuf, Vec<RawNode>> = BTreeMap::new();

    for (path, kind) in entries {
        let is_dir = matches!(kind, EntryKind::Dir);
        if let Some(parent) = path.parent() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            let has_md = matches!(kind, EntryKind::Markdown);
            children_map
                .entry(parent.to_path_buf())
                .or_default()
                .push(RawNode {
                    path: path.clone(),
                    name,
                    is_dir,
                    children: Vec::new(),
                    has_md,
                });
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
        let migration_item = if !node.is_dir && node.has_md {
            Some(MigrationCandidate {
                source_path: node.path.clone(),
                target_path: node.path.with_extension("tmt"),
            })
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
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tm_workspace_index_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn build_compacts_single_child_directory_chains() {
        let dir = temp_dir("compact");
        fs::create_dir_all(dir.join("a/b/c")).unwrap();
        fs::write(dir.join("a/b/c/doc.md"), "# Doc\n").unwrap();

        let index = WorkspaceIndex::build(&dir, &PrinterConfig::default(), &dir);
        let nodes = index.tree_nodes();

        assert_eq!(
            nodes.len(),
            2,
            "expected one compacted dir node + one file node"
        );
        assert_eq!(nodes[0].name, "a/b/c");
        assert!(nodes[0].is_dir);
        assert_eq!(nodes[1].name, "doc.md");
        assert_eq!(nodes[1].depth, 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_path_adds_updates_and_removes() {
        let dir = temp_dir("refresh");
        fs::write(dir.join("existing.tmt"), "@meta{}\n").unwrap();
        let mut index = WorkspaceIndex::build(&dir, &PrinterConfig::default(), &dir);
        assert_eq!(index.meta_paths().len(), 1);
        assert_eq!(index.migration_candidates().len(), 0);

        let new_md = dir.join("new.md");
        fs::write(&new_md, "# New\n").unwrap();
        index.refresh_path(&new_md);
        assert_eq!(index.migration_candidates().len(), 1);
        assert_eq!(
            index.migration_candidates()[0].target_path,
            dir.join("new.tmt")
        );

        fs::remove_file(&new_md).unwrap();
        index.refresh_path(&new_md);
        assert_eq!(index.migration_candidates().len(), 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rebuild_picks_up_out_of_band_changes() {
        let dir = temp_dir("rebuild");
        let mut index = WorkspaceIndex::build(&dir, &PrinterConfig::default(), &dir);
        assert_eq!(index.meta_paths().len(), 0);

        fs::write(dir.join("appeared.tmt"), "@meta{}\n").unwrap();
        index.rebuild();
        assert_eq!(index.meta_paths().len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn single_file_root_has_empty_tree_but_populated_candidates() {
        let dir = temp_dir("single_file");
        let md_path = dir.join("only.md");
        fs::write(&md_path, "# Only\n").unwrap();

        let index = WorkspaceIndex::build(&md_path, &PrinterConfig::default(), &dir);
        assert!(index.tree_nodes().is_empty());
        assert_eq!(index.migration_candidates().len(), 1);
        assert_eq!(index.migration_candidates()[0].source_path, md_path);

        let _ = fs::remove_dir_all(&dir);
    }
}
