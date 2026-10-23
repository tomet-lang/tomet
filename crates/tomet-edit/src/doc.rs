//! The root `EditDoc` managing tree nodes and IDs.

use std::collections::HashMap;

use crate::node::{AttrGroup, ChildItem, Node, NodeId, NodeKind};
use crate::op::{EditError, EditOp};

/// An editable tree model of a Tomet document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditDoc {
    nodes: HashMap<NodeId, Node>,
    root_children: Vec<ChildItem>,
    next_id: u64,
}

impl EditDoc {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocates a new unique [`NodeId`].
    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Inserts a node into the document storage.
    pub fn insert_node(&mut self, node: Node) {
        self.nodes.insert(node.id, node);
    }

    /// Adds a [`ChildItem`] (trivia or node) to the top-level document.
    pub fn add_root_child(&mut self, item: ChildItem) {
        self.root_children.push(item);
    }

    /// Returns the top-level items.
    pub fn root_children(&self) -> &[ChildItem] {
        &self.root_children
    }

    /// Returns the top-level node IDs, ignoring trivia.
    pub fn root_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.root_children.iter().filter_map(ChildItem::as_node_id)
    }

    /// Gets a reference to a node by ID.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Gets a mutable reference to a node by ID.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    /// Total number of nodes in the document.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the document contains no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Iterates over all nodes in storage.
    pub fn iter_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Mutably iterates over all nodes in storage.
    pub fn iter_nodes_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.nodes.values_mut()
    }

    /// Mutable access to the root children list.
    pub fn root_children_mut(&mut self) -> &mut Vec<ChildItem> {
        &mut self.root_children
    }

    /// Removes a node from storage by ID.
    pub fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        self.nodes.remove(&id)
    }

    /// Checks if any descendant of `id` has been marked as edited.
    pub fn has_edited_descendants(&self, id: NodeId) -> bool {
        let Some(node) = self.get(id) else {
            return false;
        };

        let children = match &node.kind {
            crate::node::NodeKind::Section { children, .. }
            | crate::node::NodeKind::List { children }
            | crate::node::NodeKind::ListItem { children, .. } => children,
            _ => return false,
        };

        for child in children {
            if let ChildItem::Node(child_id) = child {
                if let Some(child_node) = self.get(*child_id) {
                    if child_node.edited || self.has_edited_descendants(*child_id) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Returns all top-level node IDs that are [`NodeKind::BlockElement`].
    pub fn top_level_elements(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.root_nodes().filter(|id| {
            matches!(
                self.get(*id).map(|n| &n.kind),
                Some(NodeKind::BlockElement { .. })
            )
        })
    }

    /// Finds the first top-level [`NodeKind::BlockElement`] with the given name (sigil).
    pub fn find_element_by_name(&self, name: &str) -> Option<NodeId> {
        self.top_level_elements().find(|id| {
            matches!(
                self.get(*id).map(|n| &n.kind),
                Some(NodeKind::BlockElement { name: n, .. }) if n == name
            )
        })
    }

    /// Finds all top-level [`NodeKind::BlockElement`]s with the given name (sigil).
    pub fn find_elements_by_name<'a>(&'a self, name: &'a str) -> impl Iterator<Item = NodeId> + 'a {
        self.top_level_elements().filter(move |id| {
            matches!(
                self.get(*id).map(|n| &n.kind),
                Some(NodeKind::BlockElement { name: n, .. }) if n == name
            )
        })
    }

    /// Returns the top-level `::meta` element node ID, if present.
    pub fn meta_node(&self) -> Option<NodeId> {
        self.find_element_by_name("meta")
    }

    /// Ensures that a top-level `::meta` element exists.
    ///
    /// If an existing `::meta` block element is found, returns its [`NodeId`].
    /// Otherwise, inserts a new `::meta` block element at index 0 with standard
    /// spacing trivia and returns the newly allocated [`NodeId`].
    pub fn ensure_meta_node(&mut self) -> NodeId {
        if let Some(id) = self.meta_node() {
            return id;
        }

        let meta_id = self.alloc_id();
        let mut meta_node = Node::new(
            meta_id,
            NodeKind::BlockElement {
                name: "meta".to_string(),
                declaration: true,
                id_attr: None,
                args: Vec::new(),
                data: Vec::new(),
                content: None,
            },
            String::new(),
        );
        meta_node.mark_edited();

        self.insert_node(meta_node);
        self.root_children_mut().insert(0, ChildItem::Node(meta_id));

        // Insert spacing trivia after ::meta
        if self.root_children().len() > 1 {
            self.root_children_mut()
                .insert(1, ChildItem::Trivia("\n\n".to_string()));
        } else {
            self.root_children_mut()
                .insert(1, ChildItem::Trivia("\n".to_string()));
        }

        meta_id
    }

    /// Reads a property value from the top-level `::meta` block, if present.
    pub fn meta_data(&self, key: &str) -> Option<String> {
        let meta_id = self.meta_node()?;
        let node = self.get(meta_id)?;
        match &node.kind {
            NodeKind::BlockElement { data, .. } => {
                data.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
            }
            _ => None,
        }
    }

    /// Sets, updates, or deletes a key in the top-level `::meta` block's data map.
    ///
    /// If no `::meta` block exists and `value` is `Some`, a new `::meta` block is
    /// created at index 0 automatically.
    pub fn set_meta_data(
        &mut self,
        key: impl Into<String>,
        value: Option<impl Into<String>>,
    ) -> Result<(), EditError> {
        let key_str = key.into();
        let val_opt = value.map(Into::into);

        if val_opt.is_none() && self.meta_node().is_none() {
            return Ok(());
        }

        let meta_id = self.ensure_meta_node();
        self.set_attribute(meta_id, AttrGroup::Data, key_str, val_opt)
    }

    /// Finds the parent and child-item index for a given node.
    ///
    /// Returns `Some((None, pos))` for top-level root items, or `Some((Some(parent_id), pos))`
    /// for items located in a container's children list.
    pub(crate) fn find_child_location(&self, id: NodeId) -> Option<(Option<NodeId>, usize)> {
        if let Some(pos) = self
            .root_children()
            .iter()
            .position(|item| item.as_node_id() == Some(id))
        {
            return Some((None, pos));
        }

        for node in self.iter_nodes() {
            let children = match &node.kind {
                NodeKind::Section { children, .. }
                | NodeKind::List { children }
                | NodeKind::ListItem { children, .. } => children,
                _ => continue,
            };
            if let Some(pos) = children
                .iter()
                .position(|item| item.as_node_id() == Some(id))
            {
                return Some((Some(node.id), pos));
            }
        }

        None
    }

    // --- Tree navigation & hierarchy ---

    /// Returns the parent [`NodeId`] of a given node, or `None` if it is a root node
    /// or not present in the document.
    pub fn parent_of(&self, id: NodeId) -> Option<NodeId> {
        self.find_child_location(id).and_then(|(p, _)| p)
    }

    /// Returns the sequence of ancestor [`NodeId`]s starting from direct parent up to root.
    pub fn ancestors(&self, mut id: NodeId) -> Vec<NodeId> {
        let mut res = Vec::new();
        while let Some(parent) = self.parent_of(id) {
            res.push(parent);
            id = parent;
        }
        res
    }

    /// Returns the 0-based index of this node among its sibling nodes (excluding trivia).
    pub fn index_in_parent(&self, id: NodeId) -> Option<usize> {
        let siblings: Vec<NodeId> = match self.parent_of(id) {
            Some(p) => self.child_nodes(p),
            None => self.root_nodes().collect(),
        };
        siblings.iter().position(|&x| x == id)
    }

    /// Returns the raw 0-based index in the parent's `children` (or root children),
    /// including layout trivia.
    pub fn raw_index_in_parent(&self, id: NodeId) -> Option<usize> {
        self.find_child_location(id).map(|(_, pos)| pos)
    }

    /// Returns the previous sibling [`NodeId`] (excluding trivia).
    pub fn prev_sibling(&self, id: NodeId) -> Option<NodeId> {
        let siblings: Vec<NodeId> = match self.parent_of(id) {
            Some(p) => self.child_nodes(p),
            None => self.root_nodes().collect(),
        };
        let pos = siblings.iter().position(|&x| x == id)?;
        if pos > 0 {
            Some(siblings[pos - 1])
        } else {
            None
        }
    }

    /// Returns the next sibling [`NodeId`] (excluding trivia).
    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        let siblings: Vec<NodeId> = match self.parent_of(id) {
            Some(p) => self.child_nodes(p),
            None => self.root_nodes().collect(),
        };
        let pos = siblings.iter().position(|&x| x == id)?;
        if pos + 1 < siblings.len() {
            Some(siblings[pos + 1])
        } else {
            None
        }
    }

    /// Returns all leaf node IDs in document order (nodes that have no child nodes).
    pub fn leaf_nodes(&self) -> Vec<NodeId> {
        let mut leaves = Vec::new();
        for root_id in self.root_nodes() {
            self.collect_leaves(root_id, &mut leaves);
        }
        leaves
    }

    fn collect_leaves(&self, id: NodeId, leaves: &mut Vec<NodeId>) {
        let children = self.child_nodes(id);
        if children.is_empty() {
            leaves.push(id);
        } else {
            for child in children {
                self.collect_leaves(child, leaves);
            }
        }
    }

    // --- Fluent insertion & helpers ---

    /// Appends a new child node to `parent` (or at the root if `None`).
    pub fn append_child(
        &mut self,
        parent: Option<NodeId>,
        node: Node,
    ) -> Result<NodeId, EditError> {
        let id = node.id;
        let index = match parent {
            None => self.root_children().len(),
            Some(pid) => {
                let p = self.get(pid).ok_or(EditError::NodeNotFound(pid))?;
                match &p.kind {
                    NodeKind::Section { children, .. }
                    | NodeKind::List { children }
                    | NodeKind::ListItem { children, .. } => children.len(),
                    _ => return Err(EditError::NotAContainer(pid)),
                }
            }
        };
        self.apply_op(EditOp::Insert {
            parent,
            index,
            node,
        })?;
        Ok(id)
    }

    /// Appends a paragraph to `parent` (or root if `None`).
    pub fn append_paragraph(
        &mut self,
        parent: Option<NodeId>,
        text: impl Into<String>,
    ) -> Result<NodeId, EditError> {
        let id = self.alloc_id();
        let node = Node::paragraph(id, text);
        self.append_child(parent, node)
    }

    /// Inserts a paragraph at `index` under `parent` (or root if `None`).
    pub fn insert_paragraph(
        &mut self,
        parent: Option<NodeId>,
        index: usize,
        text: impl Into<String>,
    ) -> Result<NodeId, EditError> {
        let id = self.alloc_id();
        let node = Node::paragraph(id, text);
        self.apply_op(EditOp::Insert {
            parent,
            index,
            node,
        })?;
        Ok(id)
    }

    /// Appends a fenced code block to `parent` (or root if `None`).
    pub fn append_code_block(
        &mut self,
        parent: Option<NodeId>,
        lang: Option<impl Into<String>>,
        code: impl Into<String>,
    ) -> Result<NodeId, EditError> {
        let id = self.alloc_id();
        let node = Node::code_block(id, lang, code);
        self.append_child(parent, node)
    }

    /// Appends a section heading to `parent` (or root if `None`).
    pub fn append_section(
        &mut self,
        parent: Option<NodeId>,
        level: usize,
        title: impl Into<String>,
        id_attr: Option<impl Into<String>>,
    ) -> Result<NodeId, EditError> {
        let id = self.alloc_id();
        let node = Node::section(id, level, title, id_attr);
        self.append_child(parent, node)
    }

    // --- List & task item operations ---

    /// Returns whether the node represents an actionable task item
    /// (e.g. `- [ ]`, `- [x]`, `- ( )`, `- (x)`, or `@task(...)`).
    pub fn is_task_item(&self, id: NodeId) -> bool {
        self.task_state(id).is_some()
    }

    /// Returns the completion state of a task item:
    /// - `Some(true)`: completed (`- [x]`, `- (x)`, `@task(done)`, etc.)
    /// - `Some(false)`: pending / unchecked (`- [ ]`, `- ( )`, `@task(todo)`, etc.)
    /// - `None`: not a task item
    pub fn task_state(&self, id: NodeId) -> Option<bool> {
        let node = self.get(id)?;
        match &node.kind {
            NodeKind::ListItem { text, children, .. } => {
                if let Some(state) = inspect_task_text(text) {
                    return Some(state);
                }
                for child in children {
                    if let Some(cid) = child.as_node_id() {
                        if let Some(state) = self.task_state(cid) {
                            return Some(state);
                        }
                    }
                }
                None
            }
            NodeKind::BlockElement { name, args, .. } if name == "task" => inspect_task_args(args),
            _ => None,
        }
    }

    /// Sets the completion state of a task item.
    ///
    /// Modifies Markdown-style `- [ ]`/`- [x]`, Tomet value markers `- ( )`/`- (x)`,
    /// combine `-@task(...)`, or standalone `@task(...)` elements.
    pub fn set_task_state(&mut self, id: NodeId, checked: bool) -> Result<(), EditError> {
        let node = self.get(id).ok_or(EditError::NodeNotFound(id))?;
        match &node.kind {
            NodeKind::ListItem { text, children, .. } => {
                if let Some(new_text) = update_task_text(text, checked) {
                    let text_updated = new_text.clone();
                    let node_mut = self.get_mut(id).unwrap();
                    if let NodeKind::ListItem { text, children, .. } = &mut node_mut.kind {
                        *text = text_updated.clone();
                        let mut trivia_updated = false;
                        for item in children.iter_mut() {
                            if let ChildItem::Trivia(t) = item {
                                if let Some(upd) = update_task_text(t, checked) {
                                    *t = upd;
                                    trivia_updated = true;
                                }
                            }
                        }
                        if !trivia_updated
                            && children.iter().all(|c| matches!(c, ChildItem::Trivia(_)))
                        {
                            *children = vec![ChildItem::Trivia(text_updated)];
                        }
                    }

                    let child_ids: Vec<NodeId> = self
                        .get(id)
                        .map(|n| match &n.kind {
                            NodeKind::ListItem { children, .. } => {
                                children.iter().filter_map(|c| c.as_node_id()).collect()
                            }
                            _ => Vec::new(),
                        })
                        .unwrap_or_default();

                    for cid in child_ids {
                        if let Some(child_node) = self.get_mut(cid) {
                            if let Some(upd) = update_task_text(&child_node.raw, checked) {
                                child_node.raw = upd;
                                child_node.mark_edited();
                            }
                        }
                    }

                    if let Some(node) = self.get_mut(id) {
                        node.mark_edited();
                    }
                    return Ok(());
                }

                let child_ids: Vec<NodeId> =
                    children.iter().filter_map(|c| c.as_node_id()).collect();
                for cid in child_ids {
                    if self.is_task_item(cid) {
                        return self.set_task_state(cid, checked);
                    }
                }

                Err(EditError::NotAnElement(id))
            }
            NodeKind::BlockElement { name, .. } if name == "task" => {
                let mut updated = false;
                let node_mut = self.get_mut(id).unwrap();
                if let NodeKind::BlockElement { args, .. } = &mut node_mut.kind {
                    for (k, v) in args.iter_mut() {
                        if k.is_empty() || k == "status" {
                            let old = v.trim().trim_matches('"');
                            *v = if checked {
                                if old == "todo" {
                                    "done".to_string()
                                } else {
                                    "x".to_string()
                                }
                            } else if old == "done" {
                                "todo".to_string()
                            } else {
                                "todo".to_string()
                            };
                            updated = true;
                            break;
                        }
                    }
                    if !updated {
                        args.insert(
                            0,
                            (
                                String::new(),
                                if checked {
                                    "x".to_string()
                                } else {
                                    "todo".to_string()
                                },
                            ),
                        );
                    }
                }
                if let Some(node) = self.get_mut(id) {
                    node.mark_edited();
                }
                Ok(())
            }
            _ => Err(EditError::NotAnElement(id)),
        }
    }

    /// Toggles the completion state of a task item.
    pub fn toggle_task(&mut self, id: NodeId) -> Result<(), EditError> {
        let current = self.task_state(id).ok_or(EditError::NotAnElement(id))?;
        self.set_task_state(id, !current)
    }

    /// Indents a list item by adding 2 spaces of indentation.
    pub fn indent_list_item(&mut self, id: NodeId) -> Result<(), EditError> {
        let node = self.get_mut(id).ok_or(EditError::NodeNotFound(id))?;
        match &mut node.kind {
            NodeKind::ListItem { text, children, .. } => {
                text.insert_str(0, "  ");
                if let Some(ChildItem::Trivia(t)) = children.first_mut() {
                    t.insert_str(0, "  ");
                } else {
                    children.insert(0, ChildItem::Trivia("  ".to_string()));
                }
                node.mark_edited();
                Ok(())
            }
            _ => Err(EditError::NotAnElement(id)),
        }
    }

    /// Outdents a list item by removing up to 2 spaces of indentation.
    pub fn outdent_list_item(&mut self, id: NodeId) -> Result<(), EditError> {
        let node = self.get_mut(id).ok_or(EditError::NodeNotFound(id))?;
        match &mut node.kind {
            NodeKind::ListItem { text, children, .. } => {
                let spaces = if text.starts_with("  ") {
                    2
                } else if text.starts_with(' ') {
                    1
                } else {
                    0
                };
                if spaces > 0 {
                    *text = text[spaces..].to_string();
                    if let Some(ChildItem::Trivia(t)) = children.first_mut() {
                        if t.starts_with(&"  "[..spaces]) {
                            *t = t[spaces..].to_string();
                        }
                    }
                    node.mark_edited();
                }
                Ok(())
            }
            _ => Err(EditError::NotAnElement(id)),
        }
    }
}

fn inspect_task_text(text: &str) -> Option<bool> {
    let trimmed = text.trim_start();
    let after_marker = if let Some(rest) = trimmed
        .strip_prefix('-')
        .or_else(|| trimmed.strip_prefix('*'))
        .or_else(|| trimmed.strip_prefix('+'))
    {
        rest.trim_start()
    } else {
        trimmed
    };

    if after_marker.starts_with("(x)")
        || after_marker.starts_with("(X)")
        || after_marker.starts_with("(done)")
    {
        Some(true)
    } else if after_marker.starts_with("( )")
        || after_marker.starts_with("()")
        || after_marker.starts_with("(todo)")
        || after_marker.starts_with("(\" \")")
    {
        Some(false)
    } else if after_marker.starts_with("@task(x)") || after_marker.starts_with("@task(done)") {
        Some(true)
    } else if after_marker.starts_with("@task(todo)")
        || after_marker.starts_with("@task(\" \")")
        || after_marker.starts_with("@task( )")
    {
        Some(false)
    } else {
        None
    }
}

fn inspect_task_args(args: &[(String, String)]) -> Option<bool> {
    for (k, v) in args {
        if k.is_empty() || k == "status" {
            let val = v.trim().trim_matches('"');
            if val == "done" || val == "x" || val == "X" {
                return Some(true);
            } else if val == "todo" || val == " " || val.is_empty() {
                return Some(false);
            }
        }
    }
    Some(false)
}

fn update_task_text(text: &str, checked: bool) -> Option<String> {
    // Check (done) <-> (todo)
    if let Some(pos) = text.find("(todo)") {
        return if checked {
            let mut s = text.to_string();
            s.replace_range(pos..pos + 6, "(done)");
            Some(s)
        } else {
            Some(text.to_string())
        };
    }
    if let Some(pos) = text.find("(done)") {
        return if !checked {
            let mut s = text.to_string();
            s.replace_range(pos..pos + 6, "(todo)");
            Some(s)
        } else {
            Some(text.to_string())
        };
    }
    // Check () <-> (x)
    if let Some(pos) = text.find("()") {
        return if checked {
            let mut s = text.to_string();
            s.replace_range(pos..pos + 2, "(x)");
            Some(s)
        } else {
            Some(text.to_string())
        };
    }
    // Check ( ) <-> (x)
    if let Some(pos) = text.find("( )") {
        return if checked {
            let mut s = text.to_string();
            s.replace_range(pos..pos + 3, "(x)");
            Some(s)
        } else {
            Some(text.to_string())
        };
    }
    // Check (" ") <-> (x)
    if let Some(pos) = text.find("(\" \")") {
        return if checked {
            let mut s = text.to_string();
            s.replace_range(pos..pos + 5, "(x)");
            Some(s)
        } else {
            Some(text.to_string())
        };
    }
    if let Some(pos) = text.find("(x)").or_else(|| text.find("(X)")) {
        return if !checked {
            let mut s = text.to_string();
            s.replace_range(pos..pos + 3, "( )");
            Some(s)
        } else {
            Some(text.to_string())
        };
    }
    // Check @task(...)
    if text.contains("@task(") {
        let s = if checked {
            text.replace("@task(todo)", "@task(done)")
                .replace("@task(\" \")", "@task(x)")
                .replace("@task( )", "@task(x)")
        } else {
            text.replace("@task(done)", "@task(todo)")
                .replace("@task(x)", "@task(todo)")
                .replace("@task(X)", "@task(todo)")
        };
        return Some(s);
    }
    None
}
