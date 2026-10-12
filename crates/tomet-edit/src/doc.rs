//! The root `EditDoc` managing tree nodes and IDs.

use std::collections::HashMap;

use crate::node::{AttrGroup, ChildItem, Node, NodeId, NodeKind};
use crate::op::EditError;

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

    /// Returns the top-level `@meta` element node ID, if present.
    pub fn meta_node(&self) -> Option<NodeId> {
        self.find_element_by_name("meta")
    }

    /// Ensures that a top-level `@meta` element exists.
    ///
    /// If an existing `@meta` block element is found, returns its [`NodeId`].
    /// Otherwise, inserts a new `@meta` block element at index 0 with standard
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

        // Insert spacing trivia after @meta
        if self.root_children().len() > 1 {
            self.root_children_mut()
                .insert(1, ChildItem::Trivia("\n\n".to_string()));
        } else {
            self.root_children_mut()
                .insert(1, ChildItem::Trivia("\n".to_string()));
        }

        meta_id
    }

    /// Reads a property value from the top-level `@meta` block, if present.
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

    /// Sets, updates, or deletes a key in the top-level `@meta` block's data map.
    ///
    /// If no `@meta` block exists and `value` is `Some`, a new `@meta` block is
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
}
