//! In-memory bidirectional dependency graph and impact propagation engine.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::model::{AffectedPaths, Backlink, Change, Dependency, DependencyKind, DocumentNode};

/// Record of a document removed from the graph.
#[derive(Debug, Clone)]
pub struct RemovedRecord {
    pub node: DocumentNode,
    pub dependents: HashSet<String>,
}

/// Bidirectional in-memory dependency graph for document nodes.
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// Mapping from document path to its node descriptor.
    nodes: HashMap<String, DocumentNode>,
    /// Forward edges: path -> set of outgoing dependencies.
    forward: HashMap<String, HashSet<Dependency>>,
    /// Reverse edges: target -> set of referencing document paths.
    reverse: HashMap<String, HashSet<String>>,
    /// Cached incoming backlinks per document.
    backlinks: HashMap<String, Vec<Backlink>>,
}

impl DependencyGraph {
    /// Creates an empty dependency graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a dependency graph from an iterator of document nodes.
    pub fn from_nodes(nodes: impl IntoIterator<Item = DocumentNode>) -> Self {
        let mut graph = Self::new();
        for node in nodes {
            graph.insert(node);
        }
        graph
    }

    /// Returns true if the document exists in the graph.
    pub fn contains(&self, path: &str) -> bool {
        self.nodes.contains_key(path)
    }

    /// Number of tracked documents in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns true if the graph contains no documents.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the node descriptor for a document, if present.
    pub fn get_node(&self, path: &str) -> Option<&DocumentNode> {
        self.nodes.get(path)
    }

    /// Iterator over all tracked document paths.
    pub fn all_paths(&self) -> impl Iterator<Item = &String> {
        self.nodes.keys()
    }

    /// Direct reference to all document nodes in the graph.
    pub fn nodes(&self) -> &HashMap<String, DocumentNode> {
        &self.nodes
    }

    /// Outgoing dependencies from the specified document.
    pub fn dependencies_of(&self, path: &str) -> Option<&HashSet<Dependency>> {
        self.forward.get(path)
    }

    /// Referencing documents (dependents) that point to the specified document.
    pub fn dependents_of(&self, path: &str) -> Option<&HashSet<String>> {
        self.reverse.get(path)
    }

    /// Incoming backlinks for the given document.
    pub fn backlinks_to(&self, target: &str) -> &[Backlink] {
        self.backlinks.get(target).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Complete map of all backlinks.
    pub fn all_backlinks(&self) -> &HashMap<String, Vec<Backlink>> {
        &self.backlinks
    }

    /// Inserts or updates a document node, updating forward and reverse edges.
    pub fn insert(&mut self, node: DocumentNode) -> Option<DocumentNode> {
        let path = node.path.clone();
        let prev = self.nodes.get(&path).cloned();

        // Remove old forward & reverse edges if replacing
        if let Some(old_deps) = self.forward.remove(&path) {
            for dep in old_deps {
                if let Some(rev) = self.reverse.get_mut(&dep.target) {
                    rev.remove(&path);
                    if rev.is_empty() {
                        self.reverse.remove(&dep.target);
                    }
                }
            }
        }

        // Add new forward & reverse edges
        let mut new_deps = HashSet::new();
        for dep in &node.outgoing {
            new_deps.insert(dep.clone());
            self.reverse
                .entry(dep.target.clone())
                .or_default()
                .insert(path.clone());
        }
        self.forward.insert(path.clone(), new_deps);
        self.nodes.insert(path, node);

        self.rebuild_backlinks();
        prev
    }

    /// Removes a document from the graph, cleaning up its edges and dependents.
    pub fn remove(&mut self, path: &str) -> Option<RemovedRecord> {
        let node = self.nodes.remove(path)?;

        if let Some(outgoing) = self.forward.remove(path) {
            for dep in outgoing {
                if let Some(rev) = self.reverse.get_mut(&dep.target) {
                    rev.remove(path);
                    if rev.is_empty() {
                        self.reverse.remove(&dep.target);
                    }
                }
            }
        }

        let dependents = self.reverse.remove(path).unwrap_or_default();
        self.backlinks.remove(path);
        self.rebuild_backlinks();

        Some(RemovedRecord { node, dependents })
    }

    /// Rebuilds cached backlink tables across all targets.
    fn rebuild_backlinks(&mut self) {
        let mut new_backlinks: HashMap<String, Vec<Backlink>> = HashMap::new();

        for (source_path, deps) in &self.forward {
            for dep in deps {
                new_backlinks
                    .entry(dep.target.clone())
                    .or_default()
                    .push(Backlink {
                        source: source_path.clone(),
                        target: dep.target.clone(),
                        kind: dep.kind,
                    });
            }
        }

        for list in new_backlinks.values_mut() {
            list.sort_by(|a, b| a.source.cmp(&b.source));
        }

        self.backlinks = new_backlinks;
    }

    /// Computes which documents are affected given a slice of changes.
    pub fn affected_by_changes(&self, changes: &[Change]) -> AffectedPaths {
        let mut report = AffectedPaths::default();

        for change in changes {
            match change {
                Change::ContentOnly(path) => {
                    report.modified.insert(path.clone());
                    report.rebuild_required.insert(path.clone());
                }
                Change::Modified(new_node) => {
                    let path = &new_node.path;
                    if let Some(prev) = self.nodes.get(path) {
                        let content_changed = new_node.content_hash != prev.content_hash;
                        let exports_changed = new_node.exports_hash != prev.exports_hash;

                        let prev_deps: HashSet<&Dependency> = prev.outgoing.iter().collect();
                        let curr_deps: HashSet<&Dependency> = new_node.outgoing.iter().collect();

                        let added_deps: Vec<&Dependency> = new_node
                            .outgoing
                            .iter()
                            .filter(|d| !prev_deps.contains(d))
                            .collect();
                        let removed_deps: Vec<&Dependency> = prev
                            .outgoing
                            .iter()
                            .filter(|d| !curr_deps.contains(d))
                            .collect();

                        if content_changed {
                            report.modified.insert(path.clone());
                            report.rebuild_required.insert(path.clone());
                        }

                        if exports_changed {
                            report.exports_changed.insert(path.clone());
                            report.rebuild_required.insert(path.clone());

                            // Dependents pointing to this document need rebuilding
                            if let Some(dependents) = self.reverse.get(path) {
                                for dep in dependents {
                                    report.rebuild_required.insert(dep.clone());
                                }
                            }
                        }

                        if !added_deps.is_empty() || !removed_deps.is_empty() {
                            report.rebuild_required.insert(path.clone());

                            for added in added_deps {
                                report.backlinks_changed.insert(added.target.clone());
                                report.rebuild_required.insert(added.target.clone());
                            }
                            for rem in removed_deps {
                                report.backlinks_changed.insert(rem.target.clone());
                                report.rebuild_required.insert(rem.target.clone());
                            }
                        }
                    } else {
                        // Node wasn't in graph previously; treat as Added
                        report.added.insert(path.clone());
                        report.rebuild_required.insert(path.clone());
                        for dep in &new_node.outgoing {
                            report.backlinks_changed.insert(dep.target.clone());
                            report.rebuild_required.insert(dep.target.clone());
                        }
                    }
                }
                Change::Added(new_node) => {
                    let path = &new_node.path;
                    report.added.insert(path.clone());
                    report.rebuild_required.insert(path.clone());
                    for dep in &new_node.outgoing {
                        report.backlinks_changed.insert(dep.target.clone());
                        report.rebuild_required.insert(dep.target.clone());
                    }
                }
                Change::Removed(path) => {
                    report.removed.insert(path.clone());

                    // Referrers now have a broken link
                    if let Some(dependents) = self.reverse.get(path) {
                        for dep in dependents {
                            report.rebuild_required.insert(dep.clone());
                        }
                    }

                    // Targets lose a backlink
                    if let Some(deps) = self.forward.get(path) {
                        for dep in deps {
                            report.backlinks_changed.insert(dep.target.clone());
                            report.rebuild_required.insert(dep.target.clone());
                        }
                    }
                }
            }
        }

        // Transclusion / Embed propagation:
        // When document X's content changes, any document Y embedding X must rebuild.
        let mut queue: VecDeque<String> = report.modified.iter().cloned().collect();
        let mut visited: HashSet<String> = report.modified.clone();

        while let Some(changed_id) = queue.pop_front() {
            if let Some(referrers) = self.reverse.get(&changed_id) {
                for referrer_id in referrers {
                    if let Some(deps) = self.forward.get(referrer_id) {
                        let is_embedded = deps
                            .iter()
                            .any(|d| d.target == changed_id && d.kind == DependencyKind::Embed);
                        if is_embedded && visited.insert(referrer_id.clone()) {
                            report.rebuild_required.insert(referrer_id.clone());
                            queue.push_back(referrer_id.clone());
                        }
                    }
                }
            }
        }

        report
    }
}

/// Pure function: computes affected documents given all document nodes and a slice of changes.
pub fn compute_affected_paths(
    all_nodes: impl IntoIterator<Item = DocumentNode>,
    changes: &[Change],
) -> AffectedPaths {
    let graph = DependencyGraph::from_nodes(all_nodes);
    graph.affected_by_changes(changes)
}
