//! Serializes an [`EditDoc`] back into Tomet source text.

use crate::doc::EditDoc;
use crate::node::{ChildItem, NodeId, NodeKind};

/// Exports an [`EditDoc`] back to a Tomet source string.
///
/// Guaranteed Invariant: An import followed by export with no edits is byte-identical
/// to the input source:
/// `export(&import_source(src)) == src`
pub fn export_doc(doc: &EditDoc) -> String {
    let mut out = String::new();
    for item in doc.root_children() {
        export_item(doc, item, &mut out);
    }
    out
}

fn export_item(doc: &EditDoc, item: &ChildItem, out: &mut String) {
    match item {
        ChildItem::Trivia(trivia) => out.push_str(trivia),
        ChildItem::Node(id) => export_node(doc, *id, out),
    }
}

fn export_node(doc: &EditDoc, id: NodeId, out: &mut String) {
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
            id_attr,
            args,
            data,
            content,
        } => {
            out.push('@');
            out.push_str(name);
            if let Some(id) = id_attr {
                out.push_str("#(");
                out.push_str(id);
                out.push(')');
            }
            if !args.is_empty() {
                out.push('(');
                for (i, (k, v)) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    if k.is_empty() {
                        out.push_str(v);
                    } else {
                        out.push_str(k);
                        out.push_str(": ");
                        out.push_str(v);
                    }
                }
                out.push(')');
            }
            if !data.is_empty() {
                out.push('{');
                for (i, (k, v)) in data.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    if k.is_empty() {
                        out.push_str(v);
                    } else {
                        out.push_str(k);
                        out.push_str(": ");
                        out.push_str(v);
                    }
                }
                out.push('}');
            }
            if let Some(c) = content {
                out.push('[');
                out.push_str(c);
                out.push(']');
            }
            if node.raw.ends_with('\n') {
                out.push('\n');
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
