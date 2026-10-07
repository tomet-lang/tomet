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
}

/// Specific variant of an editable node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// A section with its heading and child blocks/trivia.
    Section {
        level: usize,
        heading_raw: String,
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
        children: Vec<ChildItem>,
    },
    /// A fenced code block (` ```lang ... ``` `).
    CodeBlock { lang: Option<String>, code: String },
    /// A thematic break / horizontal rule (`---`).
    ThematicBreak { raw: String },
    /// Other block kinds preserved as raw text.
    Other { kind_name: String },
}
