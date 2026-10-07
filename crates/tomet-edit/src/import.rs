//! Imports a concrete syntax tree (CST) into an [`EditDoc`].

use tomet_cst::{NodeOrToken, SyntaxKind as K, SyntaxNode};

use crate::doc::EditDoc;
use crate::node::{ChildItem, Node, NodeId, NodeKind};

/// Parses `src` into a CST and imports it into an [`EditDoc`].
pub fn import_source(src: &str) -> EditDoc {
    let root = tomet_parser::parse_cst(src);
    import_cst(&root)
}

/// Imports an existing Rowan CST [`SyntaxNode`] into an [`EditDoc`].
pub fn import_cst(root: &SyntaxNode) -> EditDoc {
    let mut doc = EditDoc::new();
    for element in root.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                doc.add_root_child(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(node) => {
                let id = import_block_node(&mut doc, &node);
                doc.add_root_child(ChildItem::Node(id));
            }
        }
    }
    doc
}

fn import_block_node(doc: &mut EditDoc, node: &SyntaxNode) -> NodeId {
    let raw = node.text().to_string();
    let id = doc.alloc_id();

    let kind = match node.kind() {
        K::SECTION => import_section(doc, node),
        K::PARAGRAPH => NodeKind::Paragraph {
            text: raw.clone(),
        },
        K::BLOCK_ELEMENT => import_block_element(node),
        K::LIST => import_list(doc, node),
        K::LIST_ITEM => import_list_item(doc, node),
        K::CODE_BLOCK => import_code_block(node),
        K::THEMATIC_BREAK => NodeKind::ThematicBreak { raw: raw.clone() },
        other => NodeKind::Other {
            kind_name: format!("{other:?}"),
        },
    };

    let node_item = Node::new(id, kind, raw);
    doc.insert_node(node_item);
    id
}

fn import_section(doc: &mut EditDoc, node: &SyntaxNode) -> NodeKind {
    let mut heading_raw = String::new();
    let mut level = 1;
    let mut children = Vec::new();

    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                children.push(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(child_node) => {
                if child_node.kind() == K::SECTION_HEADING {
                    heading_raw = child_node.text().to_string();
                    level = count_heading_level(&heading_raw);
                } else {
                    let child_id = import_block_node(doc, &child_node);
                    children.push(ChildItem::Node(child_id));
                }
            }
        }
    }

    NodeKind::Section {
        level,
        heading_raw,
        children,
    }
}

fn count_heading_level(raw: &str) -> usize {
    let count = raw.trim_start().chars().take_while(|c| *c == '=').count();
    if count == 0 { 1 } else { count }
}

fn import_block_element(node: &SyntaxNode) -> NodeKind {
    let name = node
        .children()
        .find(|c| c.kind() == K::SIGIL)
        .map(|sigil| sigil.text().to_string().trim_start_matches('@').to_string())
        .unwrap_or_default();

    let id_attr = node
        .children()
        .find(|c| c.kind() == K::ID_GROUP)
        .map(|id| {
            let t = id.text().to_string();
            t.trim_start_matches("#(").trim_end_matches(')').trim().to_string()
        });

    let args = node
        .children()
        .find(|c| c.kind() == K::ARGS)
        .map(|args_node| extract_entries(&args_node))
        .unwrap_or_default();

    let data = node
        .children()
        .find(|c| c.kind() == K::VALUE_DATA)
        .map(|val_node| extract_entries(&val_node))
        .unwrap_or_default();

    let content = node
        .children()
        .find(|c| c.kind() == K::CONTENT)
        .map(|c| {
            let t = c.text().to_string();
            let inner = t.strip_prefix('[').unwrap_or(&t);
            let inner = inner.strip_suffix(']').unwrap_or(inner);
            inner.to_string()
        });

    NodeKind::BlockElement {
        name,
        id_attr,
        args,
        data,
        content,
    }
}

fn extract_entries(group_node: &SyntaxNode) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for child in group_node.children() {
        match child.kind() {
            K::MAP_ENTRY => {
                let text = child.text().to_string();
                if let Some((k, v)) = text.split_once(':') {
                    entries.push((k.trim().to_string(), v.trim().to_string()));
                } else {
                    entries.push((text.trim().to_string(), String::new()));
                }
            }
            K::SEQ_ITEM => {
                let text = child.text().to_string().trim().to_string();
                entries.push((String::new(), text));
            }
            _ => {}
        }
    }
    entries
}

fn import_list(doc: &mut EditDoc, node: &SyntaxNode) -> NodeKind {
    let mut children = Vec::new();
    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                children.push(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(child_node) => {
                let child_id = import_block_node(doc, &child_node);
                children.push(ChildItem::Node(child_id));
            }
        }
    }
    NodeKind::List { children }
}

fn import_list_item(doc: &mut EditDoc, node: &SyntaxNode) -> NodeKind {
    let mut marker = String::new();
    let mut children = Vec::new();

    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                if marker.is_empty() && (token.kind() == K::MINUS || token.kind() == K::PLUS || token.kind() == K::STAR) {
                    marker = token.text().to_string();
                }
                children.push(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(child_node) => {
                let child_id = import_block_node(doc, &child_node);
                children.push(ChildItem::Node(child_id));
            }
        }
    }

    NodeKind::ListItem {
        marker,
        text: node.text().to_string(),
        children,
    }
}

fn import_code_block(node: &SyntaxNode) -> NodeKind {
    let code = node.text().to_string();
    NodeKind::CodeBlock {
        lang: None,
        code,
    }
}
