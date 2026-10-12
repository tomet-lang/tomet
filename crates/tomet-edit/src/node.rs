//! Node and child definitions for `tomet-edit`.

use std::fmt;

/// An opaque, unique identifier for a node in an [`EditDoc`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node:{}", self.0)
    }
}

/// An entry in a child list: either layout trivia (whitespace/comment) or a child node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildItem {
    Trivia(String),
    Node(NodeId),
}

impl ChildItem {
    pub fn as_node_id(&self) -> Option<NodeId> {
        match self {
            Self::Node(id) => Some(*id),
            Self::Trivia(_) => None,
        }
    }
}

/// Target attribute slot or group on an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttrGroup {
    /// `{ key: value, ... }` data map (e.g. in `@meta{ ... }`).
    Data,
    /// `( key: value, ... )` arguments (e.g. in `@callout(type: "info")`).
    Args,
    /// `#(id)` identity attribute slot. The `key` in `SetAttribute` is unused.
    Id,
}

/// An editable node in a Tomet document tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    /// The original source slice for this node in the CST.
    /// When `edited` is false, exporting writes this verbatim.
    pub raw: String,
    /// Whether this node has been modified since import.
    pub edited: bool,
}

impl Node {
    pub fn new(id: NodeId, kind: NodeKind, raw: String) -> Self {
        Self {
            id,
            kind,
            raw,
            edited: false,
        }
    }

    /// Mark this node as edited so export regenerates its text.
    pub fn mark_edited(&mut self) {
        self.edited = true;
    }

    /// Returns the identity attribute `#(id)` attached to this node, if any.
    pub fn id_attr(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::BlockElement { id_attr, .. }
            | NodeKind::Section { id_attr, .. }
            | NodeKind::ListItem { id_attr, .. } => id_attr.as_deref(),
            _ => None,
        }
    }

    /// Constructs a new [`NodeKind::Paragraph`] node.
    pub fn paragraph(id: NodeId, text: impl Into<String>) -> Self {
        let mut text_str = text.into();
        if !text_str.ends_with('\n') {
            text_str.push('\n');
        }
        Self {
            id,
            kind: NodeKind::Paragraph {
                text: text_str.clone(),
            },
            raw: text_str,
            edited: true,
        }
    }

    /// Constructs a new [`NodeKind::CodeBlock`] node.
    pub fn code_block(
        id: NodeId,
        lang: Option<impl Into<String>>,
        code: impl Into<String>,
    ) -> Self {
        let lang_opt = lang.map(Into::into);
        let code_str = code.into();
        let formatted = if code_str.starts_with("```") {
            code_str
        } else {
            let l = lang_opt.as_deref().unwrap_or("");
            let trimmed = code_str.trim_end_matches(['\r', '\n']);
            format!("```{l}\n{trimmed}\n```\n")
        };
        Self {
            id,
            kind: NodeKind::CodeBlock {
                lang: lang_opt,
                code: formatted.clone(),
            },
            raw: formatted,
            edited: true,
        }
    }

    /// Constructs a new [`NodeKind::Section`] node with a formatted heading.
    pub fn section(
        id: NodeId,
        level: usize,
        title: impl Into<String>,
        id_attr: Option<impl Into<String>>,
    ) -> Self {
        let level = level.max(1);
        let prefix = "=".repeat(level);
        let title_str = title.into();
        let id_opt = id_attr.map(Into::into);
        let heading_raw = match &id_opt {
            Some(id_str) => format!("{prefix}[ {title_str} ] #({id_str})\n"),
            None => format!("{prefix}[ {title_str} ]\n"),
        };
        Self {
            id,
            kind: NodeKind::Section {
                level,
                heading_raw: heading_raw.clone(),
                id_attr: id_opt,
                children: Vec::new(),
            },
            raw: heading_raw,
            edited: true,
        }
    }

    /// Constructs a new [`NodeKind::ThematicBreak`] node.
    pub fn thematic_break(id: NodeId) -> Self {
        let raw = "---\n".to_string();
        Self {
            id,
            kind: NodeKind::ThematicBreak { raw: raw.clone() },
            raw,
            edited: true,
        }
    }

    /// Constructs a new [`NodeKind::List`] node grouping child list items.
    pub fn list(id: NodeId, items: impl IntoIterator<Item = NodeId>) -> Self {
        let children = items.into_iter().map(ChildItem::Node).collect();
        Self {
            id,
            kind: NodeKind::List { children },
            raw: String::new(),
            edited: true,
        }
    }

    /// Constructs a new [`NodeKind::ListItem`] node.
    pub fn list_item(
        id: NodeId,
        marker: impl Into<String>,
        text: impl Into<String>,
        id_attr: Option<impl Into<String>>,
    ) -> Self {
        let marker = marker.into();
        let mut text_str = text.into();
        if !text_str.ends_with('\n') {
            text_str.push('\n');
        }
        let id_opt = id_attr.map(Into::into);
        let formatted = match &id_opt {
            Some(id_str) => {
                let trimmed = text_str.trim_end_matches(['\r', '\n']);
                format!("{marker} {trimmed} #({id_str})\n")
            }
            None => format!("{marker} {text_str}"),
        };
        Self {
            id,
            kind: NodeKind::ListItem {
                marker,
                text: formatted.clone(),
                id_attr: id_opt,
                children: vec![ChildItem::Trivia(formatted.clone())],
            },
            raw: formatted,
            edited: true,
        }
    }

    /// Constructs a new task [`NodeKind::ListItem`] with a Tomet value marker (`- (x)` or `- ()`).
    pub fn task_item(
        id: NodeId,
        marker: impl Into<String>,
        checked: bool,
        text: impl Into<String>,
        id_attr: Option<impl Into<String>>,
    ) -> Self {
        let marker = marker.into();
        let marker_val = if checked { "(x)" } else { "()" };
        let text_str = text.into();
        let trimmed = text_str.trim_end_matches(['\r', '\n']);
        let id_opt = id_attr.map(Into::into);
        let formatted = match &id_opt {
            Some(id_str) => format!("{marker} {marker_val} {trimmed} #({id_str})\n"),
            None => format!("{marker} {marker_val} {trimmed}\n"),
        };
        Self {
            id,
            kind: NodeKind::ListItem {
                marker,
                text: formatted.clone(),
                id_attr: id_opt,
                children: vec![ChildItem::Trivia(formatted.clone())],
            },
            raw: formatted,
            edited: true,
        }
    }

    /// Constructs a new [`NodeKind::BlockElement`] node.
    pub fn block_element(
        id: NodeId,
        name: impl Into<String>,
        id_attr: Option<impl Into<String>>,
        args: Vec<(String, String)>,
        data: Vec<(String, String)>,
        content: Option<impl Into<String>>,
    ) -> Self {
        Self {
            id,
            kind: NodeKind::BlockElement {
                name: name.into(),
                id_attr: id_attr.map(Into::into),
                args,
                data,
                content: content.map(Into::into),
            },
            raw: String::new(),
            edited: true,
        }
    }
}

/// Specific variant of an editable node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A section with its heading and child blocks/trivia.
    Section {
        level: usize,
        heading_raw: String,
        id_attr: Option<String>,
        children: Vec<ChildItem>,
    },
    /// A paragraph whose content is maintained as raw text.
    Paragraph { text: String },
    /// A block element (e.g. `@callout`, `@note`, `@table`, `@meta`).
    BlockElement {
        name: String,
        id_attr: Option<String>,
        args: Vec<(String, String)>,
        data: Vec<(String, String)>,
        content: Option<String>,
    },
    /// A list grouping list items and trivia.
    List { children: Vec<ChildItem> },
    /// A single item within a list.
    ListItem {
        marker: String,
        text: String,
        id_attr: Option<String>,
        children: Vec<ChildItem>,
    },
    /// A fenced code block (` ```lang ... ``` `).
    CodeBlock { lang: Option<String>, code: String },
    /// A thematic break / horizontal rule (`---`).
    ThematicBreak { raw: String },
    /// Other block kinds preserved as raw text.
    Other { kind_name: String },
}
