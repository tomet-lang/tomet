//! Serializes an [`EditDoc`] back into Tomet source text.

use crate::doc::EditDoc;
use crate::node::{ChildItem, ElementPiece, NodeId, NodeKind};

/// Exports an [`EditDoc`] back to a Tomet source string.
///
/// Guaranteed Invariant: An import followed by export with no edits is byte-identical
/// to the input source:
/// `export(&import_source(src)) == src`
pub fn export_doc(doc: &EditDoc) -> String {
    export_children_str(doc, doc.root_children())
}

/// Exports a single node in `doc` to a Tomet source string.
pub fn export_node_str(doc: &EditDoc, id: NodeId) -> String {
    let mut out = String::new();
    export_node(doc, id, &mut out);
    out
}

/// Exports a slice of [`ChildItem`]s in `doc` to a Tomet source string.
pub fn export_children_str(doc: &EditDoc, items: &[ChildItem]) -> String {
    let mut out = String::new();
    for item in items {
        export_item(doc, item, &mut out);
    }
    out
}

pub(crate) fn export_item(doc: &EditDoc, item: &ChildItem, out: &mut String) {
    match item {
        ChildItem::Trivia(trivia) => out.push_str(trivia),
        ChildItem::Node(id) => export_node(doc, *id, out),
    }
}

pub(crate) fn export_node(doc: &EditDoc, id: NodeId, out: &mut String) {
    let Some(node) = doc.get(id) else {
        return;
    };

    if !node.edited && !doc.has_edited_descendants(id) {
        out.push_str(&node.raw);
        return;
    }

    match &node.kind {
        NodeKind::Section {
            heading_raw,
            children,
            ..
        } => {
            out.push_str(heading_raw);
            for child in children {
                export_item(doc, child, out);
            }
        }
        NodeKind::Paragraph { text } => {
            out.push_str(text);
        }
        NodeKind::BlockElement {
            name,
            declaration,
            id_attr,
        } => {
            if !node.slots.pieces.is_empty() {
                let start_len = out.len();
                for piece in &node.slots.pieces {
                    match piece {
                        ElementPiece::Sigil(s) => out.push_str(s),
                        ElementPiece::Trivia(t) => out.push_str(t),
                        ElementPiece::Id { raw } => {
                            if node.slots.id_edited {
                                if let Some(id_str) = id_attr {
                                    out.push_str("#(");
                                    out.push_str(id_str);
                                    out.push(')');
                                }
                            } else {
                                out.push_str(raw);
                            }
                        }
                        ElementPiece::Args => {
                            if let Some(slot) = &node.slots.args_slot {
                                if !slot.edited
                                    && !slot.entries.iter().any(|e| e.is_edited())
                                {
                                    out.push_str(&slot.raw);
                                } else if !slot.entries.is_empty() {
                                    slot.export_into(out);
                                }
                            }
                        }
                        ElementPiece::Data => {
                            if let Some(slot) = &node.slots.data_slot {
                                if !slot.edited
                                    && !slot.entries.iter().any(|e| e.is_edited())
                                {
                                    out.push_str(&slot.raw);
                                } else if !slot.entries.is_empty() {
                                    slot.export_into(out);
                                }
                            }
                        }
                        ElementPiece::Content {
                            open_raw,
                            close_raw,
                            raw,
                        } => {
                            let children_edited = node
                                .slots
                                .content_children
                                .as_ref()
                                .is_some_and(|children| {
                                    children.iter().any(|c| match c {
                                        ChildItem::Node(cid) => doc.get(*cid).is_some_and(|cn| {
                                            cn.edited || doc.has_edited_descendants(*cid)
                                        }),
                                        ChildItem::Trivia(_) => false,
                                    })
                                });
                            if !node.slots.content_edited && !children_edited && !raw.is_empty() {
                                out.push_str(raw);
                            } else if let Some(children) = &node.slots.content_children {
                                out.push_str(open_raw);
                                for child in children {
                                    export_item(doc, child, out);
                                }
                                out.push_str(close_raw);
                            }
                        }
                        ElementPiece::Connect(c) => out.push_str(c),
                    }
                }
                if node.raw.ends_with('\n') && !out[start_len..].ends_with('\n') {
                    out.push('\n');
                }
            } else {
                out.push_str(if *declaration { "::" } else { "@" });
                out.push_str(name);
                if let Some(id) = id_attr {
                    out.push_str("#(");
                    out.push_str(id);
                    out.push(')');
                }
                if node.raw.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        NodeKind::List { children } => {
            for child in children {
                export_item(doc, child, out);
            }
        }
        NodeKind::ListItem { text, children, .. } => {
            if children.is_empty() {
                out.push_str(text);
            } else {
                for child in children {
                    export_item(doc, child, out);
                }
            }
        }
        NodeKind::CodeBlock { code, .. } => {
            out.push_str(code);
        }
        NodeKind::ThematicBreak { raw } => {
            out.push_str(raw);
        }
        NodeKind::Other { .. } => {
            out.push_str(&node.raw);
        }
    }
}
