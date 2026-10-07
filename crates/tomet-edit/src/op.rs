use std::fmt;

use crate::doc::EditDoc;
use crate::node::{AttrGroup, ChildItem, Node, NodeId, NodeKind};

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    NodeNotFound(NodeId),
    NotAContainer(NodeId),
    NotAnElement(NodeId),
    IndexOutOfBounds(usize),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeNotFound(id) => write!(f, "Node {id} not found"),
            Self::NotAContainer(id) => write!(f, "Parent node {id} does not support children"),
            Self::NotAnElement(id) => write!(f, "Node {id} is not an element"),
            Self::IndexOutOfBounds(idx) => write!(f, "Index {idx} out of bounds"),
        }
    }
}

impl std::error::Error for EditError {}

/// A closed set of atomic edit operations on an [`EditDoc`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// Inserts a new node into the document under `parent` (or at root if None).
    Insert {
        parent: Option<NodeId>,
        index: usize,
        node: Node,
    },
    /// Deletes a node by ID from its parent or root.
    Delete { id: NodeId },
    /// First-class move of an existing node to a new parent or new position.
    Move {
        id: NodeId,
        new_parent: Option<NodeId>,
        new_index: usize,
    },
    /// Replaces the entire text of a paragraph or code block.
    EditText { id: NodeId, new_text: String },
    /// Sets, updates, or deletes an attribute/key in an element's data, args, or id slot.
    /// If `value` is `None`, the attribute or key is removed.
    SetAttribute {
        id: NodeId,
        group: AttrGroup,
        key: String,
        value: Option<String>,
    },
}

impl EditDoc {
    /// Applies an [`EditOp`] to this document.
    pub fn apply_op(&mut self, op: EditOp) -> Result<(), EditError> {
        match op {
            EditOp::Insert {
                parent,
                index,
                node,
            } => {
                let id = node.id;
                self.insert_node(node);
                self.insert_child_ref(parent, index, ChildItem::Node(id))?;
            }
            EditOp::Delete { id } => {
                self.remove_child_ref(id)?;
                self.remove_node_recursive(id);
            }
            EditOp::Move {
                id,
                new_parent,
                new_index,
            } => {
                if !self.contains_node(id) {
                    return Err(EditError::NodeNotFound(id));
                }
                self.remove_child_ref(id)?;
                self.insert_child_ref(new_parent, new_index, ChildItem::Node(id))?;
                if let Some(node) = self.get_mut(id) {
                    node.mark_edited();
                }
                if let Some(parent_id) = new_parent {
                    if let Some(parent) = self.get_mut(parent_id) {
                        parent.mark_edited();
                    }
                }
            }
            EditOp::EditText { id, new_text } => {
                let node = self.get_mut(id).ok_or(EditError::NodeNotFound(id))?;
                match &mut node.kind {
                    NodeKind::Paragraph { text } => {
                        *text = new_text;
                    }
                    NodeKind::CodeBlock { code, .. } => {
                        *code = new_text;
                    }
                    _ => {
                        // For other nodes, update raw directly
                        node.raw = new_text;
                    }
                }
                node.mark_edited();
            }
            EditOp::SetAttribute {
                id,
                group,
                key,
                value,
            } => {
                let node = self.get_mut(id).ok_or(EditError::NodeNotFound(id))?;
                match &mut node.kind {
                    NodeKind::BlockElement {
                        id_attr,
                        args,
                        data,
                        ..
                    } => match group {
                        AttrGroup::Id => {
                            *id_attr = value;
                        }
                        AttrGroup::Data => {
                            update_entry_list(data, &key, value);
                        }
                        AttrGroup::Args => {
                            update_entry_list(args, &key, value);
                        }
                    },
                    _ => return Err(EditError::NotAnElement(id)),
                }
                node.mark_edited();
            }
        }
        Ok(())
    }

    /// Helper to set, update, or remove an attribute on an element.
    pub fn set_attribute(
        &mut self,
        id: NodeId,
        group: AttrGroup,
        key: impl Into<String>,
        value: Option<impl Into<String>>,
    ) -> Result<(), EditError> {
        self.apply_op(EditOp::SetAttribute {
            id,
            group,
            key: key.into(),
            value: value.map(Into::into),
        })
    }

    fn contains_node(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    fn insert_child_ref(
        &mut self,
        parent: Option<NodeId>,
        index: usize,
        item: ChildItem,
    ) -> Result<(), EditError> {
        match parent {
            None => {
                if index > self.root_children().len() {
                    return Err(EditError::IndexOutOfBounds(index));
                }
                self.root_children_mut().insert(index, item);
                Ok(())
            }
            Some(parent_id) => {
                let parent = self
                    .get_mut(parent_id)
                    .ok_or(EditError::NodeNotFound(parent_id))?;
                parent.mark_edited();
                match &mut parent.kind {
                    NodeKind::Section { children, .. }
                    | NodeKind::List { children }
                    | NodeKind::ListItem { children, .. } => {
                        if index > children.len() {
                            return Err(EditError::IndexOutOfBounds(index));
                        }
                        children.insert(index, item);
                        Ok(())
                    }
                    _ => Err(EditError::NotAContainer(parent_id)),
                }
            }
        }
    }

    fn remove_child_ref(&mut self, id: NodeId) -> Result<(), EditError> {
        // Try root first
        if let Some(pos) = self
            .root_children()
            .iter()
            .position(|item| item.as_node_id() == Some(id))
        {
            self.root_children_mut().remove(pos);
            return Ok(());
        }

        // Search through all container nodes
        for node in self.iter_nodes_mut() {
            let removed = match &mut node.kind {
                NodeKind::Section { children, .. }
                | NodeKind::List { children }
                | NodeKind::ListItem { children, .. } => {
                    if let Some(pos) = children
                        .iter()
                        .position(|item| item.as_node_id() == Some(id))
                    {
                        children.remove(pos);
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if removed {
                node.mark_edited();
                return Ok(());
            }
        }

        Err(EditError::NodeNotFound(id))
    }

    fn remove_node_recursive(&mut self, id: NodeId) {
        if let Some(node) = self.remove_node(id) {
            match node.kind {
                NodeKind::Section { children, .. }
                | NodeKind::List { children }
                | NodeKind::ListItem { children, .. } => {
                    for child in children {
                        if let ChildItem::Node(child_id) = child {
                            self.remove_node_recursive(child_id);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn update_entry_list(entries: &mut Vec<(String, String)>, key: &str, value: Option<String>) {
    if let Some(pos) = entries.iter().position(|(k, _)| k == key) {
        match value {
            Some(v) => entries[pos].1 = v,
            None => {
                entries.remove(pos);
            }
        }
    } else if let Some(v) = value {
        entries.push((key.to_string(), v));
    }
}
