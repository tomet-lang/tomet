//! Connects: `::name(...)` and the nameless `::{...}` / `::(...)`.
//!
//! Which are refused, which are the `#(x)` and `{a: 1}` of their owner written out, and how the
//! groups of a nameless one merge into the owner's.
//!
//! Two tuples merge in one way ([`merge_tuples`]): entry by entry, by key, and an entry written on
//! both sides is an error unless both values are tuples, which merge by the same rule. An unnamed
//! entry's key is its position among the unnamed entries, so `(a)` and `(b)` clash at position 0
//! exactly as `(k: a)` and `(k: b)` clash at `k`.

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

/// `second` merged into `first`, both found at `path` from the groups being merged.
///
/// The entries of `first` stay where they were written, then come the entries only `second`
/// has. An entry both have is merged if both values are tuples and is an error otherwise,
/// equal values included: a key is written once.
fn merge_tuples(
    first: &Tuple,
    second: &Tuple,
    path: &mut Vec<String>,
    writers: &Writers,
) -> Result<Tuple> {
    let mut merged = Vec::with_capacity(first.len() + second.len());
    for (key, value) in first.keyed() {
        let value = match second.get(&key) {
            None => value.clone(),
            Some(other) => {
                path.push(key.to_string());
                let (Value::Tuple(ours), Value::Tuple(theirs)) = (value, other) else {
                    return Err(writers.clash(path));
                };
                let value = Value::Tuple(merge_tuples(ours, theirs, path, writers)?);
                path.pop();
                value
            }
        };
        merged.push(TupleEntry::named(key, value));
    }
    for (key, value) in second.keyed() {
        if !first.contains_key(&key) {
            merged.push(TupleEntry::named(key, value.clone()));
        }
    }
    // The keys of `first`, then keys it does not have: none is repeated.
    Ok(Tuple::try_from_entries(merged).expect("merged keys are distinct"))
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
    fn clash(&self, path: &[String]) -> crate::error::Error {
        let mut message = format!("duplicate key `{}`", path.join("."));
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
fn entry_at(writer: &SyntaxNode, path: &[String]) -> Option<SyntaxNode> {
    let mut within = writer.clone();
    let mut found = None;
    for key in path {
        let holder = std::iter::once(within.clone())
            .chain(within.descendants())
            .find(|node| node.children().any(|child| is_entry(&child)))?;
        let mut unnamed_count = 0usize;
        let entry = holder.children().filter(is_entry).find(|entry| {
            let entry_key = match entry.kind() {
                K::MAP_ENTRY => crate::element::lower_map_entry(entry, false)
                    .ok()
                    .and_then(|(name, _, _)| name),
                _ => None,
            };
            let entry_key = match entry_key {
                Some(name) => name,
                None => {
                    let k = unnamed_count.to_string();
                    unnamed_count += 1;
                    k
                }
            };
            entry_key == *key
        })?;
        within = entry.clone();
        found = Some(entry);
    }
    found
}
