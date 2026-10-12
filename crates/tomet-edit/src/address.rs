//! Address-based resolution and editing operations for [`EditDoc`].
//!
//! Provides resolution of address strings (such as `#intro`, `#item(0)`,
//! `#intro.item(1)`, `#meta.icon`, etc.) to [`NodeId`]s, attribute values,
//! or text content, along with high-level mutation helpers.

use crate::doc::EditDoc;
use crate::node::{AttrGroup, ChildItem, NodeId, NodeKind};
use crate::op::{EditError, EditOp};

/// The base/root anchor target of an address path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseTarget {
    /// Identity attribute anchor (`#id`, `#(id)`, `id:id`, or `id`).
    Id(String),
    /// Zero-based top-level document node index (`#item(n)` or `item(n)`).
    RootItem(usize),
}

/// A traversal step along an address path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Child node or sequence item index (`.item(n)`).
    Item(usize),
    /// Property namespace (`.prop`).
    Prop,
    /// Property key or field name (`.key`, `.id`, `.sigil`, `.content`).
    Field(String),
}

/// A parsed intra-document address path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressPath {
    pub base: BaseTarget,
    pub steps: Vec<Step>,
}

/// Parses a Tomet address string into an [`AddressPath`].
pub fn parse_address_path(raw: &str) -> Option<AddressPath> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. If contains '#', anchor is after '#'
    let fragment = if let Some((_, anchor)) = trimmed.split_once('#') {
        anchor.trim()
    } else {
        trimmed
    };

    // 2. Strip optional 'id:' prefix
    let fragment = fragment.strip_prefix("id:").unwrap_or(fragment).trim();

    // 3. Strip optional leading '#'
    let fragment = fragment.strip_prefix('#').unwrap_or(fragment).trim();

    if fragment.is_empty() {
        return None;
    }

    // 4. Parse base target
    let (base, rest) = if let Some(stripped) = fragment.strip_prefix('(') {
        // Form: `(id)...`
        let end = stripped.find(')')?;
        let id = &stripped[..end];
        let rest = &stripped[end + 1..];
        (BaseTarget::Id(id.trim().to_string()), rest)
    } else if let Some(stripped) = fragment.strip_prefix("item(") {
        // Form: `item(N)...`
        let end = stripped.find(')')?;
        let idx: usize = stripped[..end].trim().parse().ok()?;
        let rest = &stripped[end + 1..];
        (BaseTarget::RootItem(idx), rest)
    } else {
        // Bare identifier up to next '.' or '(' or end
        let end = fragment.find('.').unwrap_or(fragment.len());
        let id = &fragment[..end];
        let rest = &fragment[end..];
        (BaseTarget::Id(id.trim().to_string()), rest)
    };

    // 5. Parse traversal steps
    let mut steps = Vec::new();
    let mut cur = rest.trim();
    while !cur.is_empty() {
        if let Some(after_dot) = cur.strip_prefix('.') {
            cur = after_dot.trim();
        } else {
            break;
        }

        if let Some(stripped) = cur.strip_prefix("item(") {
            let end = stripped.find(')')?;
            let idx: usize = stripped[..end].trim().parse().ok()?;
            steps.push(Step::Item(idx));
            cur = stripped[end + 1..].trim();
        } else if let Some(stripped) = cur.strip_prefix("prop(") {
            let end = stripped.find(')')?;
            let key = stripped[..end].trim().trim_matches('"').trim_matches('\'');
            steps.push(Step::Prop);
            steps.push(Step::Field(key.to_string()));
            cur = stripped[end + 1..].trim();
        } else if cur == "prop" || cur.starts_with("prop.") {
            steps.push(Step::Prop);
            cur = cur.strip_prefix("prop").unwrap().trim();
        } else {
            // General field name up to next '.' or '('
            let end = cur
                .find(|c: char| c == '.' || c == '(')
                .unwrap_or(cur.len());
            let field = cur[..end].trim();
            if !field.is_empty() {
                steps.push(Step::Field(field.to_string()));
            }
            cur = cur[end..].trim();
        }
    }

    Some(AddressPath { base, steps })
}

impl EditDoc {
    /// Resolves an address string to a [`NodeId`].
    ///
    /// Supports:
    /// - `#id`, `#(id)`, `id:id`, or `id` for elements/sections/items with `#(id)`
    /// - `#item(n)` or `item(n)` for n-th top-level node
    /// - `#base.item(m)` for child item indexing
    pub fn node_at(&self, addr: &str) -> Option<NodeId> {
        let path = parse_address_path(addr)?;
        let mut cur_id = match &path.base {
            BaseTarget::Id(id) => self.find_node_by_id(id)?,
            BaseTarget::RootItem(idx) => self.root_nodes().nth(*idx)?,
        };

        for step in &path.steps {
            match step {
                Step::Item(idx) => {
                    cur_id = self.child_node_at(cur_id, *idx)?;
                }
                _ => return None,
            }
        }

        Some(cur_id)
    }

    /// Resolves the owner node for an address, even if the address points to
    /// a sub-field or property (e.g. `#meta.prop.icon` or `#callout.content`).
    pub fn node_for(&self, addr: &str) -> Option<NodeId> {
        let path = parse_address_path(addr)?;
        let mut cur_id = match &path.base {
            BaseTarget::Id(id) => self.find_node_by_id(id)?,
            BaseTarget::RootItem(idx) => self.root_nodes().nth(*idx)?,
        };

        for step in &path.steps {
            match step {
                Step::Item(idx) => {
                    cur_id = self.child_node_at(cur_id, *idx)?;
                }
                _ => break,
            }
        }

        Some(cur_id)
    }

    /// Finds the node defining `id_attr == Some(id)`.
    pub fn find_node_by_id(&self, id: &str) -> Option<NodeId> {
        self.iter_nodes()
            .find(|n| n.id_attr() == Some(id))
            .map(|n| n.id)
    }

    /// Returns the non-trivia child node IDs of `parent_id`.
    pub fn child_nodes(&self, parent_id: NodeId) -> Vec<NodeId> {
        let Some(node) = self.get(parent_id) else {
            return Vec::new();
        };

        let children = match &node.kind {
            NodeKind::Section { children, .. }
            | NodeKind::List { children }
            | NodeKind::ListItem { children, .. } => children,
            _ => return Vec::new(),
        };

        children.iter().filter_map(ChildItem::as_node_id).collect()
    }

    /// Returns the `idx`-th non-trivia child node of `parent_id`.
    pub fn child_node_at(&self, parent_id: NodeId, idx: usize) -> Option<NodeId> {
        self.child_nodes(parent_id).get(idx).copied()
    }

    /// Checks whether an address resolves to an existing node.
    pub fn has_node_at(&self, addr: &str) -> bool {
        self.node_at(addr).is_some()
    }

    /// Reads text or value at `addr`.
    pub fn text_at(&self, addr: &str) -> Option<String> {
        let path = parse_address_path(addr)?;
        let mut cur_id = match &path.base {
            BaseTarget::Id(id) => self.find_node_by_id(id)?,
            BaseTarget::RootItem(idx) => self.root_nodes().nth(*idx)?,
        };

        let mut remaining_steps = path.steps.as_slice();
        while let Some(Step::Item(idx)) = remaining_steps.first() {
            cur_id = self.child_node_at(cur_id, *idx)?;
            remaining_steps = &remaining_steps[1..];
        }

        let node = self.get(cur_id)?;

        if remaining_steps.is_empty() {
            match &node.kind {
                NodeKind::Paragraph { text } => Some(text.clone()),
                NodeKind::CodeBlock { code, .. } => Some(code.clone()),
                NodeKind::Section { heading_raw, .. } => Some(heading_raw.clone()),
                NodeKind::BlockElement { content, .. } => {
                    content.clone().or_else(|| Some(node.raw.clone()))
                }
                NodeKind::ListItem { text, .. } => Some(text.clone()),
                _ => Some(node.raw.clone()),
            }
        } else {
            match remaining_steps {
                [Step::Field(f)] if f == "id" => node.id_attr().map(|s| s.to_string()),
                [Step::Field(f)] if f == "sigil" => match &node.kind {
                    NodeKind::BlockElement { name, .. } => Some(name.clone()),
                    _ => None,
                },
                [Step::Field(f)] if f == "content" => match &node.kind {
                    NodeKind::BlockElement { content, .. } => content.clone(),
                    NodeKind::Paragraph { text } => Some(text.clone()),
                    _ => None,
                },
                [Step::Prop, Step::Field(key)] | [Step::Field(key)] => match &node.kind {
                    NodeKind::BlockElement { data, args, .. } => data
                        .iter()
                        .find(|(k, _)| k == key)
                        .or_else(|| args.iter().find(|(k, _)| k == key))
                        .map(|(_, v)| v.clone()),
                    _ => None,
                },
                _ => None,
            }
        }
    }

    /// Sets text of the node at `addr`.
    pub fn set_text_at(
        &mut self,
        addr: &str,
        new_text: impl Into<String>,
    ) -> Result<(), EditError> {
        let text = new_text.into();
        let path =
            parse_address_path(addr).ok_or_else(|| EditError::AddressNotFound(addr.to_string()))?;

        // Check if addr targets `.content` specifically on a BlockElement
        if let [.., Step::Field(f)] = path.steps.as_slice() {
            if f == "content" {
                let owner_id = self
                    .node_for(addr)
                    .ok_or_else(|| EditError::AddressNotFound(addr.to_string()))?;
                let node = self
                    .get_mut(owner_id)
                    .ok_or(EditError::NodeNotFound(owner_id))?;
                if let NodeKind::BlockElement { content, .. } = &mut node.kind {
                    *content = Some(text);
                    node.mark_edited();
                    return Ok(());
                }
            }
        }

        let node_id = self
            .node_at(addr)
            .ok_or_else(|| EditError::AddressNotFound(addr.to_string()))?;
        self.apply_op(EditOp::EditText {
            id: node_id,
            new_text: text,
        })
    }

    /// Deletes the node at `addr`.
    pub fn delete_at(&mut self, addr: &str) -> Result<(), EditError> {
        let node_id = self
            .node_at(addr)
            .ok_or_else(|| EditError::AddressNotFound(addr.to_string()))?;
        self.apply_op(EditOp::Delete { id: node_id })
    }

    /// Sets or removes an attribute on the node at `addr`.
    pub fn set_attribute_at(
        &mut self,
        addr: &str,
        group: AttrGroup,
        key: impl Into<String>,
        value: Option<impl Into<String>>,
    ) -> Result<(), EditError> {
        let node_id = self
            .node_at(addr)
            .ok_or_else(|| EditError::AddressNotFound(addr.to_string()))?;
        self.set_attribute(node_id, group, key, value)
    }

    /// Sets or removes a property value by address (e.g. `#meta.icon` or `#callout.type`).
    pub fn set_prop_at(
        &mut self,
        addr: &str,
        value: Option<impl Into<String>>,
    ) -> Result<(), EditError> {
        let path =
            parse_address_path(addr).ok_or_else(|| EditError::AddressNotFound(addr.to_string()))?;
        let key = match path.steps.as_slice() {
            [.., Step::Prop, Step::Field(k)] | [.., Step::Field(k)] => k.clone(),
            _ => return Err(EditError::AddressNotFound(addr.to_string())),
        };

        let node_id = self
            .node_for(addr)
            .ok_or_else(|| EditError::AddressNotFound(addr.to_string()))?;
        let node = self.get(node_id).ok_or(EditError::NodeNotFound(node_id))?;

        let group = match &node.kind {
            NodeKind::BlockElement { args, data, .. } => {
                if args.iter().any(|(k, _)| k == &key) {
                    AttrGroup::Args
                } else if data.iter().any(|(k, _)| k == &key) {
                    AttrGroup::Data
                } else if args.is_empty() && !data.is_empty() {
                    AttrGroup::Data
                } else if !args.is_empty() && data.is_empty() {
                    AttrGroup::Args
                } else {
                    AttrGroup::Data
                }
            }
            _ => return Err(EditError::NotAnElement(node_id)),
        };

        self.set_attribute(node_id, group, key, value)
    }
}
