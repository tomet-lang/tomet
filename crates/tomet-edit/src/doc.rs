//! The root `EditDoc` managing tree nodes and IDs.

use std::collections::HashMap;

use crate::node::{ChildItem, Node, NodeId};

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
}
