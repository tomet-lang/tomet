//! Documents, directories and the parts inside documents.

use tomet_ast::{Block, Connect, File, Group, Value};

use super::args::Args;
use super::context::EvalContext;
use super::error::EvalError;
use super::methods::index_into;
use super::value::{DirRef, DocRef, EvalValue, Part};

/// The document behind a reference.
pub(crate) fn load(doc: &DocRef, cx: &EvalContext) -> Result<std::sync::Arc<File>, EvalError> {
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

/// The directory a path is in; the vault root (`""`) for a path with no `/`.
fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The documents under `dir` that `keep` lets through, as a list.
fn directory_docs(
    dir: &DirRef,
    cx: &EvalContext,
    keep: impl Fn(&DocRef) -> bool,
) -> Result<EvalValue, EvalError> {
    cx.resolver
        .dir_docs(dir)
        .map(|docs| {
            EvalValue::Seq(
                docs.into_iter()
                    .filter(|doc| keep(doc))
                    .map(EvalValue::Document)
                    .collect(),
            )
        })
        .map_err(|why| {
            // the vault root is the empty path: nothing to quote
            if dir.0.is_empty() {
                EvalError::Unresolved(why)
            } else {
                EvalError::Unresolved(format!("directory `{}`: {why}", dir.0))
            }
        })
}

// ---- finding a part by id ----------------------------------------------------

/// The part with unique id `id`: an element, a declaration, a section, a list or a list item,
/// wherever it sits in the document, in a `::connect` too.
pub(crate) fn find_part(doc: &File, id: &str) -> Option<Part> {
    let found = tomet_tree::walk(doc, &mut |node, _| {
        if node.id().is_some_and(|i| i.0 == id)
            && let Some(found) = Part::from_tree_node(node)
        {
            return tomet_tree::Visit::Break(found);
        }
        tomet_tree::Visit::Descend
    });
    match found {
        std::ops::ControlFlow::Break(part) => Some(part),
        std::ops::ControlFlow::Continue(()) => None,
    }
}

impl Part {
    /// The expression language's part for a node of the tree, if it has one. A bare entry carries an
    /// id but is not something an expression can name.
    pub fn from_tree_node(node: tomet_tree::Node<'_>) -> Option<Part> {
        if let Some(el) = node.element() {
            return Some(Part::Element(Box::new(el.clone())));
        }
        if let Some(list) = node.list() {
            return Some(Part::List(Box::new(list.clone())));
        }
        match node {
            tomet_tree::Node::Block(Block::Section(s)) => Some(Part::Section(Box::new(s.clone()))),
            tomet_tree::Node::Declaration(d) => Some(Part::Declaration(Box::new(d.clone()))),
            tomet_tree::Node::Item(item) => Some(Part::Item(Box::new(item.clone()))),
            _ => None,
        }
    }
}

/// `doc#id`.
pub(crate) fn part_by_id(doc: &DocRef, id: &str, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let document = load(doc, cx)?;
    find_part(&document, id)
        .map(|part| EvalValue::Part {
            doc: doc.clone(),
            part: Box::new(part),
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
    let blocks: Vec<EvalValue> = document.body.iter().map(|b| block_value(doc, b)).collect();
    index_into(&blocks, index)
}

fn block_value(doc: &DocRef, block: &Block) -> EvalValue {
    match block {
        Block::Element(el) => EvalValue::Part {
            doc: doc.clone(),
            part: Box::new(Part::Element(Box::new(el.clone()))),
        },
        Block::Section(sec) => EvalValue::Part {
            doc: doc.clone(),
            part: Box::new(Part::Section(Box::new(sec.clone()))),
        },
        Block::List(list) => EvalValue::Part {
            doc: doc.clone(),
            part: Box::new(Part::List(Box::new(list.clone()))),
        },
        other => EvalValue::Fragment(vec![other.clone()]),
    }
}

// ---- members of a part -------------------------------------------------------

/// An element's `(args)` and `{value}` seen as one map: the value's keys win
/// on a conflict, since `{value}` is where an element's real data usually
/// lives (`::meta(...)`) while `(args)` tends toward attributes. When neither is
/// a map, whichever scalar group there is.
fn prop(doc: &DocRef, args: &Option<Value>, value: &Option<Group>) -> EvalValue {
    let args_map = match args {
        Some(Value::Map(entries)) => Some(entries.clone()),
        _ => None,
    };
    let value_data = value.as_ref().map(|v| v.as_data());
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

/// What a part holds, as blocks: an element's or a section's body, and for a
/// list the list itself, since its items are its body.
fn child_blocks(part: &Part) -> Vec<Block> {
    match part {
        Part::Element(el) => el.content.clone().unwrap_or_default(),
        Part::Connect(conn) => conn.content.clone().unwrap_or_default(),
        Part::Declaration(d) => d.content.clone().unwrap_or_default(),
        Part::Item(item) => item.content.clone(),
        Part::Section(sec) => sec.blocks.clone(),
        Part::List(list) => vec![Block::List((**list).clone())],
    }
}

/// The things `.item(n)` counts in a part: the blocks of an element or a
/// section, and the items of a list. An item is an element, so it is a part
/// like any other (its `#(id)` is found by `#id` as well).
fn children(doc: &DocRef, part: &Part) -> Vec<EvalValue> {
    match part {
        Part::Element(_)
        | Part::Connect(_)
        | Part::Declaration(_)
        | Part::Item(_)
        | Part::Section(_) => child_blocks(part)
            .iter()
            .map(|b| block_value(doc, b))
            .collect(),
        Part::List(list) => list
            .items
            .iter()
            .map(|item| EvalValue::Part {
                doc: doc.clone(),
                part: Box::new(Part::Item(Box::new(item.clone()))),
            })
            .collect(),
    }
}

pub(crate) fn part_member(doc: &DocRef, part: &Part, member: &str) -> Result<EvalValue, EvalError> {
    match member {
        "prop" => Ok(match part {
            Part::Element(el) => prop(doc, &el.data.args, &el.data.value),
            Part::Section(sec) => prop(doc, &sec.data.args, &sec.data.value),
            Part::Connect(conn) => prop(doc, &conn.data.args, &conn.data.value),
            Part::Declaration(d) => prop(doc, &d.data.args, &d.data.value),
            Part::Item(item) => prop(doc, &item.data.args, &item.data.value),
            // A list has neither arguments nor a value group.
            Part::List(_) => prop(doc, &None, &None),
        }),
        "id" => Ok(part
            .id()
            .map(|id| EvalValue::String(id.to_string()))
            .unwrap_or(EvalValue::Null)),
        "name" => Ok(EvalValue::String(match part {
            Part::Element(el) => el.name.to_string(),
            Part::Connect(conn) => conn.name.to_string(),
            Part::Declaration(d) => d.name.to_string(),
            Part::Item(item) => item
                .name
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            Part::Section(_) => "section".to_string(),
            Part::List(_) => "list".to_string(),
        })),
        "connects" => {
            // A connect takes no connects of its own.
            let connects: &[Connect] = match part {
                Part::Element(el) => &el.connects,
                Part::Section(sec) => &sec.connects,
                Part::List(list) => &list.connects,
                Part::Item(item) => &item.connects,
                Part::Connect(_) | Part::Declaration(_) => &[],
            };
            Ok(EvalValue::Seq(
                connects
                    .iter()
                    .map(|conn| EvalValue::Part {
                        doc: doc.clone(),
                        part: Box::new(Part::Connect(Box::new(conn.clone()))),
                    })
                    .collect(),
            ))
        }
        "content" => Ok(EvalValue::Fragment(child_blocks(part))),
        _ => Err(EvalError::UnknownMember {
            on: "element",
            member: member.to_string(),
        }),
    }
}

/// Calls on a part: `.item(n)` and `.prop(key)`.
pub(crate) fn part_method(
    doc: &DocRef,
    part: &Part,
    member: &str,
    args: Args,
) -> Result<EvalValue, EvalError> {
    match member {
        "link" => {
            args.exactly(".link", 0)?;
            super::fragments::link_fragment(&EvalValue::Part {
                doc: doc.clone(),
                part: Box::new(part.clone()),
            })
        }
        "item" => {
            let [index]: [EvalValue; 1] = args
                .exactly(".item", 1)?
                .try_into()
                .unwrap_or_else(|_| unreachable!("`exactly` returned one value"));
            index_into(&children(doc, part), &index)
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
            match part_member(doc, part, "prop")? {
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
        "kind" => Ok(tomet_declaration::document_kind(&*load(doc, cx)?)
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
        "docs" => directory_docs(dir, cx, |_| true),
        "children" => directory_docs(dir, cx, |doc| parent_dir(&doc.0) == dir.0),
        _ => Err(EvalError::UnknownMember {
            on: "directory",
            member: member.to_string(),
        }),
    }
}
