//! Documents, directories and the nodes inside documents.

use tomet_ast::{Block, Document, Element, ElementValue, Entry, Inline, List, Value};

use super::args::Args;
use super::context::EvalContext;
use super::error::EvalError;
use super::methods::index_into;
use super::value::{DirRef, DocRef, EvalValue, Node};

/// The document behind a reference.
pub(crate) fn load(doc: &DocRef, cx: &EvalContext) -> Result<std::sync::Arc<Document>, EvalError> {
    cx.resolver
        .document(doc)
        .map_err(|why| EvalError::Unresolved(format!("document `{}`: {why}", doc.0)))
}

/// Strips a leading `./`, so `./a.tmt` and `a.tmt` are the same document.
pub(crate) fn normalize_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

/// `self.filename` and the like: the last segment of the path.
fn filename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

// ---- finding a node by id ----------------------------------------------------

/// The node with unique id `id`: an element, a section or a list.
pub(crate) fn find_node(doc: &Document, id: &str) -> Option<Node> {
    doc.blocks.iter().find_map(|block| in_block(block, id))
}

fn in_block(block: &Block, id: &str) -> Option<Node> {
    match block {
        Block::Section(sec) => {
            if sec.id.as_ref().is_some_and(|i| i.0 == id) {
                return Some(Node::Section(Box::new(sec.clone())));
            }
            sec.connects
                .iter()
                .find_map(|conn| in_element(conn, id))
                .or_else(|| sec.blocks.iter().find_map(|b| in_block(b, id)))
        }
        Block::Element(el) => in_element(el, id),
        Block::Paragraph(p) => p.content.iter().find_map(|inline| match inline {
            Inline::Element(el) => in_element(el, id),
            _ => None,
        }),
        Block::List(list) => in_list(list, id),
    }
}

fn in_list(list: &List, id: &str) -> Option<Node> {
    if list.id.as_ref().is_some_and(|i| i.0 == id) {
        return Some(Node::List(Box::new(list.clone())));
    }
    list.connects
        .iter()
        .find_map(|conn| in_element(conn, id))
        .or_else(|| {
            list.items.iter().find_map(|item| {
                in_element(&item.element, id)
                    .or_else(|| item.sublist.as_ref().and_then(|sub| in_list(sub, id)))
            })
        })
}

fn in_element(el: &Element, id: &str) -> Option<Node> {
    if el.id.as_ref().is_some_and(|i| i.0 == id) {
        return Some(Node::Element(Box::new(el.clone())));
    }
    el.content
        .iter()
        .flatten()
        .find_map(|block| in_block(block, id))
        .or_else(|| el.connects.iter().find_map(|conn| in_element(conn, id)))
        .or_else(|| match &el.value {
            Some(ElementValue::Group(entries)) => entries.iter().find_map(|entry| match entry {
                Entry::Element(child) => in_element(child, id),
                _ => None,
            }),
            _ => None,
        })
}

/// `doc#id`.
pub(crate) fn node_by_id(doc: &DocRef, id: &str, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let document = load(doc, cx)?;
    find_node(&document, id)
        .map(|node| EvalValue::Node {
            doc: doc.clone(),
            node: Box::new(node),
        })
        .ok_or_else(|| EvalError::Unresolved(format!("no element with id `{id}` in `{}`", doc.0)))
}

/// `#item(n)`: the n-th block at the root of the document.
pub(crate) fn root_item(
    doc: &DocRef,
    index: &EvalValue,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    let document = load(doc, cx)?;
    let blocks: Vec<EvalValue> = document
        .blocks
        .iter()
        .map(|b| block_value(doc, b))
        .collect();
    index_into(&blocks, index)
}

fn block_value(doc: &DocRef, block: &Block) -> EvalValue {
    match block {
        Block::Element(el) => EvalValue::Node {
            doc: doc.clone(),
            node: Box::new(Node::Element(Box::new(el.clone()))),
        },
        Block::Section(sec) => EvalValue::Node {
            doc: doc.clone(),
            node: Box::new(Node::Section(Box::new(sec.clone()))),
        },
        Block::List(list) => EvalValue::Node {
            doc: doc.clone(),
            node: Box::new(Node::List(Box::new(list.clone()))),
        },
        other => EvalValue::Fragment(vec![other.clone()]),
    }
}

// ---- members of a node -------------------------------------------------------

/// An element's `(args)` and `{value}` seen as one map: the value's keys win
/// on a conflict, since `{value}` is where an element's real data usually
/// lives (`@meta{...}`) while `(args)` tends toward attributes. When neither is
/// a map, whichever scalar group there is.
fn prop(doc: &DocRef, args: &Option<Value>, value: &Option<ElementValue>) -> EvalValue {
    let args_map = match args {
        Some(Value::Map(entries)) => Some(entries.clone()),
        _ => None,
    };
    let value_data = value.as_ref().and_then(|v| v.as_data());
    let value_map = match &value_data {
        Some(Value::Map(entries)) => Some(entries.clone()),
        _ => None,
    };
    let merged = match (args_map, value_map) {
        (Some(mut merged), Some(from_value)) => {
            for (key, val) in from_value {
                match merged.iter_mut().find(|(k, _)| *k == key) {
                    Some(existing) => existing.1 = val,
                    None => merged.push((key, val)),
                }
            }
            Value::Map(merged)
        }
        (Some(args), None) => Value::Map(args),
        (None, Some(from_value)) => Value::Map(from_value),
        (None, None) => value_data.or_else(|| args.clone()).unwrap_or(Value::Null),
    };
    EvalValue::from_ast(&merged, doc)
}

/// What a node holds, as blocks: an element's or a section's body, and for a
/// list the list itself, since its items are its body.
fn child_blocks(node: &Node) -> Vec<Block> {
    match node {
        Node::Element(el) => el.content.clone().unwrap_or_default(),
        Node::Section(sec) => sec.blocks.clone(),
        Node::List(list) => vec![Block::List((**list).clone())],
    }
}

/// The things `.item(n)` counts in a node: the blocks of an element or a
/// section, and the items of a list. An item is an element, so it is a node
/// like any other (its `#(id)` is found by `#id` as well).
fn children(doc: &DocRef, node: &Node) -> Vec<EvalValue> {
    match node {
        Node::Element(_) | Node::Section(_) => child_blocks(node)
            .iter()
            .map(|b| block_value(doc, b))
            .collect(),
        Node::List(list) => list
            .items
            .iter()
            .map(|item| EvalValue::Node {
                doc: doc.clone(),
                node: Box::new(Node::Element(Box::new(item.element.clone()))),
            })
            .collect(),
    }
}

pub(crate) fn node_member(doc: &DocRef, node: &Node, member: &str) -> Result<EvalValue, EvalError> {
    match member {
        "prop" => Ok(match node {
            Node::Element(el) => prop(doc, &el.args, &el.value),
            Node::Section(sec) => prop(doc, &sec.args, &sec.value),
            // A list has neither arguments nor a value group.
            Node::List(_) => prop(doc, &None, &None),
        }),
        "id" => Ok(node
            .id()
            .map(|id| EvalValue::String(id.to_string()))
            .unwrap_or(EvalValue::Null)),
        "sigil" => Ok(EvalValue::String(match node {
            Node::Element(el) => el.sigil.name().map(|n| n.to_string()).unwrap_or_default(),
            Node::Section(_) => "section".to_string(),
            Node::List(_) => "list".to_string(),
        })),
        "connects" => {
            let connects = match node {
                Node::Element(el) => &el.connects,
                Node::Section(sec) => &sec.connects,
                Node::List(list) => &list.connects,
            };
            Ok(EvalValue::Seq(
                connects
                    .iter()
                    .map(|conn| EvalValue::Node {
                        doc: doc.clone(),
                        node: Box::new(Node::Element(Box::new(conn.clone()))),
                    })
                    .collect(),
            ))
        }
        "content" => Ok(EvalValue::Fragment(child_blocks(node))),
        _ => Err(EvalError::UnknownMember {
            on: "element",
            member: member.to_string(),
        }),
    }
}

/// Calls on a node: `.item(n)` and `.prop(key)`.
pub(crate) fn node_method(
    doc: &DocRef,
    node: &Node,
    member: &str,
    args: Args,
) -> Result<EvalValue, EvalError> {
    match member {
        "link" => {
            args.exactly(".link", 0)?;
            super::fragments::link_fragment(&EvalValue::Node {
                doc: doc.clone(),
                node: Box::new(node.clone()),
            })
        }
        "item" => {
            let [index]: [EvalValue; 1] = args
                .exactly(".item", 1)?
                .try_into()
                .unwrap_or_else(|_| unreachable!("`exactly` returned one value"));
            index_into(&children(doc, node), &index)
        }
        "prop" => {
            let key = match args.exactly(".prop", 1)?.pop() {
                Some(EvalValue::String(key)) => key,
                Some(other) => {
                    return Err(EvalError::mismatch(
                        "`.prop`",
                        "a string key",
                        other.type_name(),
                    ));
                }
                None => unreachable!("`exactly` returned one value"),
            };
            match node_member(doc, node, "prop")? {
                EvalValue::Map(entries) => entries
                    .into_iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v)
                    .ok_or(EvalError::UnknownMember {
                        on: "map",
                        member: key,
                    }),
                other => Err(EvalError::UnknownMember {
                    on: other.type_name(),
                    member: key,
                }),
            }
        }
        _ => Err(EvalError::UnknownMember {
            on: "element",
            member: member.to_string(),
        }),
    }
}

// ---- members of a document and a directory -----------------------------------

pub(crate) fn document_member(
    doc: &DocRef,
    member: &str,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    match member {
        "path" => Ok(EvalValue::String(doc.0.clone())),
        "filename" => Ok(EvalValue::String(filename(&doc.0).to_string())),
        "meta" => cx
            .resolver
            .meta(doc)
            .map_err(|why| EvalError::Unresolved(format!("metadata of `{}`: {why}", doc.0))),
        "kind" => Ok(tomet_semantics::document_kind(&*load(doc, cx)?)
            .map(EvalValue::String)
            .unwrap_or(EvalValue::Null)),
        _ => Err(EvalError::UnknownMember {
            on: "document",
            member: member.to_string(),
        }),
    }
}

pub(crate) fn document_method(
    doc: &DocRef,
    member: &str,
    args: Args,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    match member {
        "item" => {
            let [index]: [EvalValue; 1] = args
                .exactly(".item", 1)?
                .try_into()
                .unwrap_or_else(|_| unreachable!("`exactly` returned one value"));
            root_item(doc, &index, cx)
        }
        "link" => {
            args.exactly(".link", 0)?;
            super::fragments::link_fragment(&EvalValue::Document(doc.clone()))
        }
        _ => Err(EvalError::UnknownMember {
            on: "document",
            member: member.to_string(),
        }),
    }
}

pub(crate) fn directory_member(
    dir: &DirRef,
    member: &str,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    match member {
        "docs" => cx
            .resolver
            .dir_docs(dir)
            .map(|docs| EvalValue::Seq(docs.into_iter().map(EvalValue::Document).collect()))
            .map_err(|why| EvalError::Unresolved(format!("directory `{}`: {why}", dir.0))),
        _ => Err(EvalError::UnknownMember {
            on: "directory",
            member: member.to_string(),
        }),
    }
}
