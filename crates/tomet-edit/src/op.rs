use std::fmt;

use crate::doc::EditDoc;
use crate::node::{AttrGroup, AttrSlot, ChildItem, ElementPiece, Node, NodeId, NodeKind};

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    NodeNotFound(NodeId),
    NotAContainer(NodeId),
    NotAnElement(NodeId),
    IndexOutOfBounds(usize),
    AddressNotFound(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeNotFound(id) => write!(f, "Node {id} not found"),
            Self::NotAContainer(id) => write!(f, "Parent node {id} does not support children"),
            Self::NotAnElement(id) => write!(f, "Node {id} is not an element"),
            Self::IndexOutOfBounds(idx) => write!(f, "Index {idx} out of bounds"),
            Self::AddressNotFound(addr) => write!(f, "Address '{addr}' could not be resolved"),
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
    /// Replaces an existing node by ID with a new node in-place.
    Replace { id: NodeId, new_node: Node },
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
            EditOp::Replace { id, new_node } => {
                let (parent, pos) = self
                    .find_child_location(id)
                    .ok_or(EditError::NodeNotFound(id))?;
                let new_id = new_node.id;
                self.insert_node(new_node);

                match parent {
                    None => {
                        self.root_children_mut()[pos] = ChildItem::Node(new_id);
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
                                children[pos] = ChildItem::Node(new_id);
                            }
                            NodeKind::BlockElement { .. } => {
                                let children = parent
                                    .slots
                                    .content_children
                                    .as_mut()
                                    .ok_or(EditError::NotAContainer(parent_id))?;
                                children[pos] = ChildItem::Node(new_id);
                            }
                            _ => return Err(EditError::NotAContainer(parent_id)),
                        }
                    }
                }

                self.remove_node_recursive(id);
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
                if let Some(parent_id) = new_parent
                    && let Some(parent) = self.get_mut(parent_id) {
                        parent.mark_edited();
                    }
            }
            EditOp::EditText { id, new_text } => {
                let node = self.get_mut(id).ok_or(EditError::NodeNotFound(id))?;
                match &mut node.kind {
                    NodeKind::Paragraph { text } => {
                        node.inlines = crate::import::parse_inline_items(&new_text);
                        *text = new_text;
                    }
                    NodeKind::CodeBlock { code, .. } => {
                        *code = new_text;
                    }
                    NodeKind::Section { heading_raw, .. } => {
                        *heading_raw = new_text;
                    }
                    NodeKind::ListItem { text, children, .. } => {
                        *text = new_text.clone();
                        if children.iter().all(|c| matches!(c, ChildItem::Trivia(_))) {
                            *children = vec![ChildItem::Trivia(new_text)];
                        } else if let Some(ChildItem::Trivia(t)) = children.first_mut() {
                            *t = new_text;
                        }
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
                let existing_id_child = if group == AttrGroup::Id {
                    self.child_nodes(id)
                        .into_iter()
                        .find(|cid| self.get(*cid).is_some_and(|n| n.raw.starts_with("#(")))
                } else {
                    None
                };

                let node = self.get_mut(id).ok_or(EditError::NodeNotFound(id))?;

                match &mut node.kind {
                    NodeKind::BlockElement {
                        name,
                        declaration,
                        id_attr,
                    } => {
                        if node.slots.pieces.is_empty() {
                            let prefix = if *declaration { "::" } else { "@" };
                            node.slots
                                .pieces
                                .push(ElementPiece::Sigil(format!("{prefix}{name}")));
                        }
                        match group {
                            AttrGroup::Id => {
                                *id_attr = value.clone();
                                node.slots.id_edited = true;
                                if value.is_some() {
                                    node.slots.ensure_piece_id();
                                }
                            }
                            AttrGroup::Data => {
                                node.slots.ensure_piece_data();
                                let slot = node
                                    .slots
                                    .data_slot
                                    .get_or_insert_with(|| AttrSlot::new_empty("{", "}"));
                                slot.set_key(&key, value.clone());
                            }
                            AttrGroup::Args => {
                                node.slots.ensure_piece_args();
                                let slot = node
                                    .slots
                                    .args_slot
                                    .get_or_insert_with(|| AttrSlot::new_empty("(", ")"));
                                slot.set_key(&key, value.clone());
                            }
                        }
                    }
                    NodeKind::Section {
                        id_attr,
                        heading_raw,
                        ..
                    } => match group {
                        AttrGroup::Id => {
                            *id_attr = value.clone();
                            *heading_raw = update_heading_id(heading_raw, value.as_deref());
                        }
                        _ => return Err(EditError::NotAnElement(id)),
                    },
                    NodeKind::ListItem {
                        id_attr,
                        text,
                        children,
                        ..
                    } => match group {
                        AttrGroup::Id => {
                            *id_attr = value.clone();
                            *text = update_item_text_id(text, value.as_deref());

                            if let Some(cid) = existing_id_child {
                                if value.is_none()
                                    && let Some(pos) =
                                        children.iter().position(|c| c.as_node_id() == Some(cid))
                                    {
                                        children.remove(pos);
                                    }
                            } else if let Some(new_id) = value.as_deref() {
                                let insert_pos = if children.last().is_some_and(|c| match c {
                                    ChildItem::Trivia(t) => t.ends_with('\n'),
                                    _ => false,
                                }) {
                                    children.len() - 1
                                } else {
                                    children.len()
                                };
                                children
                                    .insert(insert_pos, ChildItem::Trivia(format!(" #({new_id})")));
                            }
                        }
                        _ => return Err(EditError::NotAnElement(id)),
                    },
                    _ => return Err(EditError::NotAnElement(id)),
                }

                if let Some(node) = self.get_mut(id) {
                    node.mark_edited();
                }

                if let Some(cid) = existing_id_child
                    && let Some(new_id) = value.as_deref()
                        && let Some(child_node) = self.get_mut(cid) {
                            child_node.raw = format!("#({new_id})");
                            child_node.mark_edited();
                        }
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

    /// Replaces an existing node by ID with a new node in-place.
    pub fn replace_node(&mut self, id: NodeId, new_node: Node) -> Result<(), EditError> {
        self.apply_op(EditOp::Replace { id, new_node })
    }

    /// Applies a sequence of edit operations in order, stopping at the first error.
    pub fn apply_ops(&mut self, ops: impl IntoIterator<Item = EditOp>) -> Result<(), EditError> {
        for op in ops {
            self.apply_op(op)?;
        }
        Ok(())
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
                    NodeKind::BlockElement { .. } => {
                        parent.slots.ensure_piece_content();
                        let children = parent.slots.content_children.get_or_insert_with(Vec::new);
                        if index > children.len() {
                            return Err(EditError::IndexOutOfBounds(index));
                        }
                        children.insert(index, item);
                        parent.slots.content_edited = true;
                        Ok(())
                    }
                    _ => Err(EditError::NotAContainer(parent_id)),
                }
            }
        }
    }

    fn remove_child_ref(&mut self, id: NodeId) -> Result<(), EditError> {
        let (parent, pos) = self
            .find_child_location(id)
            .ok_or(EditError::NodeNotFound(id))?;

        match parent {
            None => {
                remove_child_at(self.root_children_mut(), pos);
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
                        remove_child_at(children, pos);
                        Ok(())
                    }
                    NodeKind::BlockElement { .. } => {
                        let children = parent
                            .slots
                            .content_children
                            .as_mut()
                            .ok_or(EditError::NotAContainer(parent_id))?;
                        remove_child_at(children, pos);
                        parent.slots.content_edited = true;
                        Ok(())
                    }
                    _ => Err(EditError::NotAContainer(parent_id)),
                }
            }
        }
    }

    fn remove_node_recursive(&mut self, id: NodeId) {
        if let Some(node) = self.remove_node(id) {
            let children_opt = match node.kind {
                NodeKind::Section { children, .. }
                | NodeKind::List { children }
                | NodeKind::ListItem { children, .. } => Some(children),
                NodeKind::BlockElement { .. } => node.slots.content_children,
                _ => None,
            };
            if let Some(children) = children_opt {
                for child in children {
                    if let ChildItem::Node(child_id) = child {
                        self.remove_node_recursive(child_id);
                    }
                }
            }
        }
    }
}

fn remove_child_at(children: &mut Vec<ChildItem>, pos: usize) {
    children.remove(pos);
    if pos < children.len() && matches!(children[pos], ChildItem::Trivia(_)) {
        if pos == 0 || (pos > 0 && matches!(children[pos - 1], ChildItem::Trivia(_))) {
            children.remove(pos);
        }
    } else if pos == children.len() && pos > 0 && matches!(children[pos - 1], ChildItem::Trivia(_))
        && pos - 1 > 0 && matches!(children[pos - 2], ChildItem::Trivia(_)) {
            children.remove(pos - 1);
        }
}

pub(crate) fn update_heading_id(heading_raw: &str, new_id: Option<&str>) -> String {
    let (body, newline) = if let Some(stripped) = heading_raw.strip_suffix("\r\n") {
        (stripped, "\r\n")
    } else if let Some(stripped) = heading_raw.strip_suffix('\n') {
        (stripped, "\n")
    } else {
        (heading_raw, "")
    };

    let updated_body = if let Some(start) = body.find("#(") {
        if let Some(end) = body[start..].find(')') {
            let end_idx = start + end + 1;
            match new_id {
                Some(id) => {
                    format!("{}#({}){}", &body[..start], id, &body[end_idx..])
                }
                None => {
                    let before = body[..start].trim_end();
                    let after = body[end_idx..].trim_start();
                    if after.is_empty() {
                        before.to_string()
                    } else {
                        format!("{} {}", before, after)
                    }
                }
            }
        } else {
            body.to_string()
        }
    } else {
        match new_id {
            Some(id) => {
                let trimmed = body.trim_end();
                format!("{}#({})", trimmed, id)
            }
            None => body.to_string(),
        }
    };

    format!("{updated_body}{newline}")
}

pub(crate) fn update_item_text_id(text: &str, new_id: Option<&str>) -> String {
    if let Some(start) = text.find("#(") {
        if let Some(end) = text[start..].find(')') {
            let end_idx = start + end + 1;
            match new_id {
                Some(id) => {
                    format!("{}#({}){}", &text[..start], id, &text[end_idx..])
                }
                None => {
                    let before = text[..start].trim_end();
                    let after = text[end_idx..].trim_start();
                    if after.is_empty() {
                        before.to_string()
                    } else {
                        format!("{} {}", before, after)
                    }
                }
            }
        } else {
            text.to_string()
        }
    } else {
        match new_id {
            Some(id) => {
                let has_newline = text.ends_with('\n');
                let trimmed = text.trim_end_matches(&['\r', '\n'][..]);
                if has_newline {
                    format!("{} #({})\n", trimmed, id)
                } else {
                    format!("{} #({})", trimmed, id)
                }
            }
            None => text.to_string(),
        }
    }
}
