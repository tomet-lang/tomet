//! Documents, directories and the parts inside documents.

use tomet_ast::{Block, Connect, File, Group, Tuple, Value};
use tomet_scalar::Scalar;

use tomet_expr::{DirRef, DocRef, EvalContext, EvalError, EvalValue, Part};

use super::args::Args;
use super::methods::index_into;

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
            EvalValue::Tuple(Tuple::from_unnamed(
                docs.into_iter()
                    .filter(|doc| keep(doc))
                    .map(EvalValue::Document),
            ))
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

/// `doc#id`.
pub(crate) fn part_by_id(doc: &DocRef, id: &str, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let document = load(doc, cx)?;
    tomet_query::find_by_id_with(&document, id, Part::from_tree_node)
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

/// The data a node holds -- its `(args)` and `{value}` -- seen as one tuple: the value's keys win
/// on a conflict, since `{value}` is where a node's real data usually
/// lives (`::meta(...)`) while `(args)` tends toward attributes. When neither is
/// a tuple, whichever scalar group there is.
fn data(doc: &DocRef, args: &Option<Tuple>, value: &Option<Group>) -> EvalValue {
    let value_tuple = value.as_ref().map(|v| v.as_data());
    let merged = match (args, value_tuple) {
        (Some(args_tuple), Some(val_tuple)) => {
            let mut merged = args_tuple.clone();
            for (key, value) in val_tuple.keyed() {
                merged.set(&key, value.clone());
            }
            Value::Tuple(merged)
        }
        (Some(args_tuple), None) => Value::Tuple(args_tuple.clone()),
        (None, Some(val_tuple)) => Value::Tuple(val_tuple),
        (None, None) => Value::Scalar(Scalar::None),
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

/// What a part holds as data, whatever the part is: `u1.id` or `u1.0` reads a field of this.
fn part_field(doc: &DocRef, part: &Part, member: &str) -> Option<EvalValue> {
    match part {
        Part::Element(el) => {
            if let Some(val) = el
                .data
                .value
                .as_ref()
                .and_then(|v| v.as_data().get(member).cloned())
            {
                return Some(EvalValue::from_ast(&val, doc));
            }
            if let Some(view) = tomet_semantics::element_args_view(el.as_view())
                && let Some(val) = view.get(member)
            {
                return Some(EvalValue::from_ast(val, doc));
            }
            el.data
                .args
                .as_ref()
                .and_then(|args| args.get(member))
                .map(|v| EvalValue::from_ast(v, doc))
        }
        Part::Declaration(d) => {
            if let Some(val) = d
                .data
                .value
                .as_ref()
                .and_then(|v| v.as_data().get(member).cloned())
            {
                return Some(EvalValue::from_ast(&val, doc));
            }
            if let Some(view) = tomet_semantics::declaration_args_view(d)
                && let Some(val) = view.get(member)
            {
                return Some(EvalValue::from_ast(val, doc));
            }
            d.data
                .args
                .as_ref()
                .and_then(|args| args.get(member))
                .map(|v| EvalValue::from_ast(v, doc))
        }
        Part::Section(sec) => {
            if let Some(val) = sec
                .data
                .value
                .as_ref()
                .and_then(|v| v.as_data().get(member).cloned())
            {
                return Some(EvalValue::from_ast(&val, doc));
            }
            sec.data
                .args
                .as_ref()
                .and_then(|args| args.get(member))
                .map(|v| EvalValue::from_ast(v, doc))
        }
        Part::Connect(conn) => {
            if let Some(val) = conn
                .data
                .value
                .as_ref()
                .and_then(|v| v.as_data().get(member).cloned())
            {
                return Some(EvalValue::from_ast(&val, doc));
            }
            conn.data
                .args
                .as_ref()
                .and_then(|args| args.get(member))
                .map(|v| EvalValue::from_ast(v, doc))
        }
        Part::Item(item) => {
            if let Some(val) = item
                .data
                .value
                .as_ref()
                .and_then(|v| v.as_data().get(member).cloned())
            {
                return Some(EvalValue::from_ast(&val, doc));
            }
            item.data
                .args
                .as_ref()
                .and_then(|args| args.get(member))
                .map(|v| EvalValue::from_ast(v, doc))
        }
        Part::List(_) => None,
    }
}

/// `part.key`: a key of the data the part holds, in its `(...)` or `{...}`.
///
/// The keys are the author's, so `@user(id: "u123")` has an `id` that is not the node's own
/// (`part::id`). A key the part does not have is an error, which for the retired spellings that read
/// the node's own properties gives the way to what was probably meant.
pub(crate) fn part_member(doc: &DocRef, part: &Part, member: &str) -> Result<EvalValue, EvalError> {
    if let Some(value) = part_field(doc, part, member) {
        return Ok(value);
    }
    match member {
        "prop" => Err(retired_prop()),
        "id" | "name" | "content" | "connects" => Err(EvalError::message(format!(
            "this element has no data key `{member}`; the node's own `{member}` is read with \
             `::{member}`, as `node::{member}`"
        ))),
        _ => Err(EvalError::UnknownMember {
            on: "element",
            member: member.to_string(),
        }),
    }
}

/// The error for `.prop` and `.prop(key)`, a retired level between a node and its data.
fn retired_prop() -> EvalError {
    EvalError::Retired {
        written: ".prop".to_string(),
        instead: "read a data argument directly with `.key` (`node.title`), and a node's own \
                  properties with `::` (`node::id`, `node::meta`)"
            .to_string(),
    }
}

/// `part::member`: from a node to what hangs off it. The node's own `id`, `name`, `content` and
/// `connects`; the `doc` it is in; its `meta`, which is its `{...}` (`@user(id: "u1"){ tag: vip }` is
/// `@user(id: "u1")::meta(tag: vip)`, and the tuple is empty when there is none); or the connect of
/// that name stacked after it (`u1::rule`), as the tuple of its data.
pub(crate) fn part_connect(
    doc: &DocRef,
    part: &Part,
    member: &str,
) -> Result<EvalValue, EvalError> {
    match member {
        "id" => Ok(part
            .id()
            .map(|id| EvalValue::Scalar(Scalar::Text(id.to_string())))
            .unwrap_or(EvalValue::Scalar(Scalar::None))),
        "name" => Ok(EvalValue::Scalar(Scalar::Text(match part {
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
        }))),
        "content" => Ok(EvalValue::Fragment(child_blocks(part))),
        "doc" => Ok(EvalValue::Document(doc.clone())),
        "meta" => Ok(part_meta(doc, part)),
        "connects" => Ok(EvalValue::Tuple(Tuple::from_unnamed(
            connects_of(part).iter().map(|conn| EvalValue::Part {
                doc: doc.clone(),
                part: Box::new(Part::Connect(Box::new(conn.clone()))),
            }),
        ))),
        _ => connects_of(part)
            .iter()
            .find(|conn| conn.name.is_bare_named(member))
            .map(|conn| data(doc, &conn.data.args, &conn.data.value))
            .ok_or_else(|| EvalError::UnknownMember {
                on: "element",
                member: member.to_string(),
            }),
    }
}

/// The `{...}` a part carries, as a tuple: empty when it has none.
fn part_meta(doc: &DocRef, part: &Part) -> EvalValue {
    let group = match part {
        Part::Element(el) => &el.data.value,
        Part::Section(sec) => &sec.data.value,
        Part::Connect(conn) => &conn.data.value,
        Part::Declaration(d) => &d.data.value,
        Part::Item(item) => &item.data.value,
        Part::List(_) => &None,
    };
    match group {
        Some(group) => EvalValue::from_ast(&Value::Tuple(group.as_data()), doc),
        None => EvalValue::Tuple(Tuple::new()),
    }
}

/// The connects stacked after a part. A connect takes none of its own.
fn connects_of(part: &Part) -> &[Connect] {
    match part {
        Part::Element(el) => &el.connects,
        Part::Section(sec) => &sec.connects,
        Part::List(list) => &list.connects,
        Part::Item(item) => &item.connects,
        Part::Connect(_) | Part::Declaration(_) => &[],
    }
}

/// Calls on a part: `.item(n)` and `.link(label)`.
pub(crate) fn part_method(
    doc: &DocRef,
    part: &Part,
    member: &str,
    args: Args,
) -> Result<EvalValue, EvalError> {
    match member {
        "link" => {
            let mut args = args.between(".link", 0, 1)?;
            let label = args.pop();
            super::fragments::link_fragment(
                &EvalValue::Part {
                    doc: doc.clone(),
                    part: Box::new(part.clone()),
                },
                label.as_ref(),
            )
        }
        "item" => {
            let [index]: [EvalValue; 1] = args
                .exactly(".item", 1)?
                .try_into()
                .unwrap_or_else(|_| unreachable!("`exactly` returned one value"));
            index_into(&children(doc, part), &index)
        }
        "prop" => Err(retired_prop()),
        _ => Err(EvalError::UnknownMember {
            on: "element",
            member: member.to_string(),
        }),
    }
}

/// Strips only one final line break (`\n` or `\r\n`) from the very end of the text.
fn strip_final_line_break(s: &str) -> &str {
    if let Some(stripped) = s.strip_suffix("\r\n") {
        stripped
    } else if let Some(stripped) = s.strip_suffix('\n') {
        stripped
    } else {
        s
    }
}

// ---- members of a document and a directory -----------------------------------

/// `doc.member`: a document holds no data of its own to read with a dot. Its path, filename, meta,
/// kind and source are reached with `::`, and the error says so.
pub(crate) fn document_member(doc: &DocRef, member: &str) -> Result<EvalValue, EvalError> {
    let _ = doc;
    match member {
        "path" | "filename" | "meta" | "kind" | "source" => Err(moved_to_connect(member)),
        _ => Err(EvalError::UnknownMember {
            on: "document",
            member: member.to_string(),
        }),
    }
}

/// The error for a member that is reached with `::` and not with a dot.
fn moved_to_connect(member: &str) -> EvalError {
    EvalError::Retired {
        written: format!(".{member}"),
        instead: format!(
            "`.` reads the data keys of a node, and `::` reaches what hangs off it; write `::{member}`"
        ),
    }
}

/// `doc::member`: the document's `path`, `filename`, `source`, `meta`, `kind` and `id`; `doc` is the document
/// itself, as it is for every node in it.
pub(crate) fn document_connect(
    doc: &DocRef,
    member: &str,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    match member {
        "path" => Ok(EvalValue::Scalar(Scalar::Text(doc.0.clone()))),
        "filename" => Ok(EvalValue::Scalar(Scalar::Text(
            filename(&doc.0).to_string(),
        ))),
        "meta" => cx
            .resolver
            .meta(doc)
            .map_err(|why| EvalError::Unresolved(format!("metadata of `{}`: {why}", doc.0))),
        "kind" => Ok(tomet_declaration::document_kind(&*load(doc, cx)?)
            .map(|s| EvalValue::Scalar(Scalar::Text(s.to_string())))
            .unwrap_or(EvalValue::Scalar(Scalar::None))),
        "id" => Ok(tomet_declaration::document_id(&*load(doc, cx)?)
            .map(|s| EvalValue::Scalar(Scalar::Text(s.to_string())))
            .unwrap_or(EvalValue::Scalar(Scalar::None))),
        "doc" => Ok(EvalValue::Document(doc.clone())),
        "source" => {
            let raw = cx
                .resolver
                .source(doc)
                .map_err(|why| EvalError::Unresolved(format!("document `{}`: {why}", doc.0)))?;
            let stripped = strip_final_line_break(&raw);
            Ok(EvalValue::Scalar(Scalar::Text(stripped.to_string())))
        }
        _ => Err(EvalError::UnknownMember {
            on: "document",
            member: member.to_string(),
        }),
    }
}

/// Calls on a document: `.item(n)` and `.link(label)`.
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
            let mut args = args.between(".link", 0, 1)?;
            let label = args.pop();
            super::fragments::link_fragment(&EvalValue::Document(doc.clone()), label.as_ref())
        }
        _ => Err(EvalError::UnknownMember {
            on: "document",
            member: member.to_string(),
        }),
    }
}

/// `dir.member`: a directory holds no data to read with a dot; see [`document_member`].
pub(crate) fn directory_member(dir: &DirRef, member: &str) -> Result<EvalValue, EvalError> {
    let _ = dir;
    match member {
        "docs" | "children" => Err(moved_to_connect(member)),
        _ => Err(EvalError::UnknownMember {
            on: "directory",
            member: member.to_string(),
        }),
    }
}

/// `dir::member`: the documents under the directory (`docs`), or directly in it (`children`).
pub(crate) fn directory_connect(
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
