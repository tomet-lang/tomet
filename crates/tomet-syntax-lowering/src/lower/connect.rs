//! Connects: `::name(...)` and the nameless `::{...}` / `::(...)`.
//!
//! Which are refused, which are the `#(x)` and `{a: 1}` of their owner written out, and how the
//! groups of a nameless one merge into the owner's.
//!
//! Two tuples merge in one way ([`merge_tuples`]): entry by entry, by key, and an entry written on
//! both sides is an error unless both values are tuples, which merge by the same rule. An unnamed
//! entry's key is its position among the unnamed entries, so `(a)` and `(b)` clash at position 0
//! exactly as `(k: a)` and `(k: b)` clash at `k`.

use std::fmt;

use super::form::require_primary_group;
use super::groups::lower_groups;
use super::{name_of, range_of, unsupported};
use crate::element::Groups;
use crate::error::Result;
use crate::value::err;
use tomet_ast::{Connect, Entry, Tuple, TupleEntry, Value};
use tomet_cst::{SyntaxKind as K, SyntaxNode};
use tomet_span::Span;

/// A connect is `::name(...)`, and the nameless one `::{...}` / `::(...)`: `:name(...)` and
/// `:{...}` are retired, because `:` only separates `key: value`. The CST still keeps one as
/// a connect so that this can say what to write instead.
pub(super) fn refuse_single_colon(node: &SyntaxNode) -> Result<()> {
    let colons = node
        .children_with_tokens()
        .take_while(|e| e.kind() == K::COLON)
        .count();
    if colons >= 2 {
        return Ok(());
    }
    let (start, _) = range_of(node);
    let written = node
        .children()
        .find(|n| n.kind() == K::NAME)
        .map(|n| n.text().to_string());
    let message = match written {
        Some(name) => format!(
            "a connect is written `::{name}(...)`, not `:{name}(...)`: `:` only separates `key: value`"
        ),
        None => "a nameless connect is written `::{...}` or `::(...)`, not `:{...}` or `:(...)`: \
                 `:` only separates `key: value`"
            .to_string(),
    };
    Err(err(start, message))
}

/// `::name(args){value}[content]`.
pub(super) fn lower_connect(node: &SyntaxNode) -> Result<Connect> {
    let (start, _) = range_of(node);
    let name = name_of(node).map_err(|_| unsupported("a connect without a name"))?;
    if node.children().any(|n| n.kind() == K::ID_GROUP) {
        return Err(err(
            start,
            "a `::connect` takes no `#(id)`: write the id of its owner before the first `::connect`",
        ));
    }
    let written = node
        .children()
        .find(|n| n.kind() == K::NAME)
        .map(|n| n.text().to_string())
        .unwrap_or_default();
    require_primary_group(node, &format!("::{written}"), start)?;
    let read = lower_groups(node)?;
    let end = node
        .children()
        .map(|n| range_of(&n).1)
        .max()
        .unwrap_or(start + 1);
    Ok(Connect {
        name,
        data: read.data,
        content: read.content,
        span: Span::new(start, end),
    })
}

/// `::(args)` or `::{value}`: no name, so it is not a connect of its own but more
/// of the owner's args or value.
pub(super) fn is_merge_connect(node: &SyntaxNode) -> bool {
    !node.children().any(|c| c.kind() == K::NAME)
}

/// Merges the groups of a nameless `::` connect into `read`, as the AST parser does.
pub(super) fn merge_connect(node: &SyntaxNode, read: &mut Groups) -> Result<()> {
    for child in node.children() {
        match child.kind() {
            K::ARGS => {
                let args = crate::element::lower_paren_value(&child)?;
                read.data.args = Some(match read.data.args.take() {
                    None => args,
                    Some(first) => merge_tuples(
                        &first,
                        &args,
                        &mut Vec::new(),
                        &Writers {
                            earlier: &read.args_writers,
                            second: &child,
                        },
                    )?,
                });
                read.args_writers.push(child);
            }
            K::VALUE_DATA | K::MAP_LITERAL => {
                let value = crate::element::lower_value_group(&child)?;
                merge_value_group(read, value, &child)?;
            }
            K::CONTENT => {
                return Err(err(
                    range_of(&child).0,
                    "a nameless `::` connect merges `(args)` or `{value}` into its owner; \
                     `[content]` needs a name: write `::name[...]`",
                ));
            }
            other => return Err(unsupported(format!("{other:?} in a nameless connect"))),
        }
    }
    Ok(())
}

/// Merges `value`, written by `writer`, into the `{value}` group `read` already has.
///
/// The `key: value` pairs of the two groups merge as tuples do ([`merge_tuples`]). An entry that
/// is not a pair (an element, a hole) has no key, so it clashes with nothing: the entries of the
/// first group stay where they were written, and what only the second has comes after them.
fn merge_value_group(
    read: &mut Groups,
    value: tomet_ast::Group,
    writer: &SyntaxNode,
) -> Result<()> {
    let merged = match read.data.value.take() {
        None => value,
        Some(first) => {
            let pairs = merge_tuples(
                &first.as_data(),
                &value.as_data(),
                &mut Vec::new(),
                &Writers {
                    earlier: &read.value_writers,
                    second: writer,
                },
            )?;
            let in_first = |key: &str| first.pairs().any(|(k, _)| k == key);
            let added: Vec<Entry> = value
                .0
                .iter()
                .filter(|entry| !matches!(entry, Entry::Pair(key, _) if in_first(key)))
                .cloned()
                .collect();
            let kept = first.0.iter().map(|entry| match entry {
                Entry::Pair(key, _) => match pairs.get(key) {
                    Some(value) => Entry::Pair(key.clone(), value.clone()),
                    None => entry.clone(),
                },
                Entry::Element(_) | Entry::Hole(_) => entry.clone(),
            });
            tomet_ast::Group(kept.chain(added).collect())
        }
    };
    read.data.value = Some(merged);
    read.value_writers.push(writer.clone());
    Ok(())
}

/// `::id(x)` is the `#(x)` of its owner and `::meta(a: 1)` its `{a: 1}`, written out: the
/// sugars stand for these two connects (see the design in
/// `tmtroot/dev/design/nexus-brackets-and-singular-declarations.tmt`). Whether `node` was one
/// of them, and so read into `read` instead of being a connect of its own.
pub(super) fn desugar_connect(node: &SyntaxNode, read: &mut Groups) -> Result<bool> {
    let written = node
        .children()
        .find(|n| n.kind() == K::NAME)
        .map(|n| n.text().to_string())
        .unwrap_or_default();
    if written != "id" && written != "meta" {
        return Ok(false);
    }
    let connect = lower_connect(node)?;
    let (at, _) = range_of(node);
    if connect.content.is_some() || connect.data.value.is_some() {
        return Err(err(
            at,
            format!("`::{written}` takes its `(...)` only: write `::{written}(...)`"),
        ));
    }
    let args = connect.data.args.unwrap_or_default();
    if written == "id" {
        let id = if args.len() == 1 && args.has_no_named() {
            args.nth_unnamed(0).and_then(Value::scalar_text)
        } else {
            None
        };
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            return Err(err(at, "`::id` takes one id: write `::id(x)`"));
        };
        if read.id.is_some() {
            return Err(err(at, "an id is already written for this owner"));
        }
        read.id = Some(tomet_ast::Id(id.into_owned()));
    } else {
        if !args.has_no_unnamed() {
            return Err(err(
                at,
                "`::meta` takes `key: value` pairs: write `::meta(a: 1)`",
            ));
        }
        merge_value_group(read, tomet_ast::Group::from_map(args), node)?;
    }
    Ok(true)
}

/// One step of the way to an entry: its name, or its position among the unnamed entries of
/// its tuple.
#[derive(Clone, PartialEq)]
enum Key {
    Name(String),
    Position(usize),
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Name(name) => f.write_str(name),
            Key::Position(n) => write!(f, "{n}"),
        }
    }
}

/// `second` merged into `first`, both found at `path` from the groups being merged.
///
/// The entries of `first` stay where they were written, then come the entries only `second`
/// has. An entry both have is merged if both values are tuples and is an error otherwise,
/// equal values included: a key is written once.
fn merge_tuples(
    first: &Tuple,
    second: &Tuple,
    path: &mut Vec<Key>,
    writers: &Writers,
) -> Result<Tuple> {
    let mut merged = Vec::with_capacity(first.len() + second.len());
    let mut position = 0;
    for entry in first.entries() {
        let (key, other) = match entry.name() {
            Some(name) => (Key::Name(name.to_string()), second.get(name)),
            None => {
                position += 1;
                (
                    Key::Position(position - 1),
                    second.nth_unnamed(position - 1),
                )
            }
        };
        let Some(other) = other else {
            merged.push(entry.clone());
            continue;
        };
        path.push(key);
        let (Value::Tuple(ours), Value::Tuple(theirs)) = (entry.value(), other) else {
            return Err(writers.clash(path));
        };
        let value = Value::Tuple(merge_tuples(ours, theirs, path, writers)?);
        path.pop();
        merged.push(TupleEntry::new(entry.name().map(str::to_string), value));
    }
    let unnamed_in_first = position;
    let mut position = 0;
    for entry in second.entries() {
        let only_in_second = match entry.name() {
            Some(name) => !first.contains_name(name),
            None => {
                position += 1;
                position > unnamed_in_first
            }
        };
        if only_in_second {
            merged.push(entry.clone());
        }
    }
    Ok(Tuple::from_entries(merged))
}

/// Who wrote the two tuples being merged: the nodes that wrote the first one, in the order
/// they were read, and the node that writes the second. Kept as nodes because a value has no
/// position; they are only looked into when a clash has to say where its two writers are.
struct Writers<'a> {
    earlier: &'a [SyntaxNode],
    second: &'a SyntaxNode,
}

impl Writers<'_> {
    /// The error for the entry at `path` being written twice: at the second writer, naming
    /// the place of the first.
    fn clash(&self, path: &[Key]) -> crate::error::Error {
        let written: Vec<String> = path.iter().map(Key::to_string).collect();
        let what = match path.last() {
            Some(Key::Position(_)) => "entry",
            _ => "key",
        };
        let mut message = format!("duplicate {what} `{}`", written.join("."));
        if let Some(first) = self.earlier.iter().find_map(|node| entry_at(node, path)) {
            let (offset, _) = range_of(&first);
            if let Some(root) = first.ancestors().last() {
                let source = root.text().to_string();
                let at = tomet_position::LineIndex::new(&source).line_col(offset);
                message.push_str(&format!(": first written at {}:{}", at.line, at.column));
            }
        }
        let second = entry_at(self.second, path).unwrap_or_else(|| self.second.clone());
        err(range_of(&second).0, message)
    }
}

fn is_entry(node: &SyntaxNode) -> bool {
    matches!(node.kind(), K::MAP_ENTRY | K::SEQ_ITEM)
}

/// The node of the entry at `path` in what `writer` wrote, if it wrote one there.
fn entry_at(writer: &SyntaxNode, path: &[Key]) -> Option<SyntaxNode> {
    let mut within = writer.clone();
    let mut found = None;
    for key in path {
        let holder = std::iter::once(within.clone())
            .chain(within.descendants())
            .find(|node| node.children().any(|child| is_entry(&child)))?;
        let mut position = 0;
        let entry = holder.children().filter(is_entry).find(|entry| {
            let name = match entry.kind() {
                K::MAP_ENTRY => crate::element::lower_map_entry(entry)
                    .ok()
                    .and_then(|(name, _, _)| name),
                _ => None,
            };
            match (name, key) {
                (Some(name), Key::Name(wanted)) => name == *wanted,
                (None, Key::Position(wanted)) => {
                    position += 1;
                    position - 1 == *wanted
                }
                _ => false,
            }
        })?;
        within = entry.clone();
        found = Some(entry);
    }
    found
}
