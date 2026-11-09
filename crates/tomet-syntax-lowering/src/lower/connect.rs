//! Connects: `::name(...)` and the nameless `::{...}` / `::(...)`.
//!
//! Which are refused, which are the `#(x)` and `{a: 1}` of their owner written out, and how the
//! groups of a nameless one merge into the owner's.

use super::form::require_primary_group;
use super::groups::lower_groups;
use super::{name_of, range_of, unsupported};
use crate::element::Groups;
use crate::error::Result;
use crate::value::err;
use tomet_ast::{Connect, Entry, Value};
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
                read.data.args = merge_values(read.data.args.as_ref(), Some(&args));
            }
            K::VALUE_DATA | K::MAP_LITERAL => {
                let value = crate::element::lower_value_group(&child)?;
                merge_value_group(read, value);
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

/// Merges `value` into the `{value}` group `read` already has, if both hold only pairs.
fn merge_value_group(read: &mut Groups, value: tomet_ast::Group) {
    let merged = match &read.data.value {
        Some(existing)
            if existing.0.iter().all(|e| matches!(e, Entry::Pair(..)))
                && value.0.iter().all(|e| matches!(e, Entry::Pair(..))) =>
        {
            merge_values(Some(&existing.as_data()), Some(&value.as_data()))
                .map(tomet_ast::Group::from_map)
        }
        _ => None,
    };
    read.data.value = Some(merged.unwrap_or(value));
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
    let args = connect.data.args.unwrap_or(Value::Map(Vec::new()));
    if written == "id" {
        let id = match &args {
            Value::Map(_) | Value::Seq(_) => None,
            scalar => scalar.scalar_text(),
        };
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            return Err(err(at, "`::id` takes one id: write `::id(x)`"));
        };
        if read.id.is_some() {
            return Err(err(at, "an id is already written for this owner"));
        }
        read.id = Some(tomet_ast::Id(id.into_owned()));
    } else {
        let Value::Map(entries) = args else {
            return Err(err(
                at,
                "`::meta` takes `key: value` pairs: write `::meta(a: 1)`",
            ));
        };
        if entries.iter().any(|(k, _)| k.is_empty()) {
            return Err(err(
                at,
                "`::meta` takes `key: value` pairs: write `::meta(a: 1)`",
            ));
        }
        merge_value_group(read, tomet_ast::Group::from_map(Value::Map(entries)));
    }
    Ok(true)
}

/// `connected` merged under `direct`: maps key by key (the direct value wins a
/// clash), sequences without repeating an item, anything else the direct one.
fn merge_values(direct: Option<&Value>, connected: Option<&Value>) -> Option<Value> {
    match (direct, connected) {
        (None, None) => None,
        (Some(d), None) => Some(d.clone()),
        (None, Some(c)) => Some(c.clone()),
        (Some(d), Some(c)) => Some(merge_values_inner(d, c)),
    }
}

fn merge_values_inner(direct: &Value, connected: &Value) -> Value {
    match (direct, connected) {
        (Value::Map(d_map), Value::Map(c_map)) => {
            let mut merged = c_map.clone();
            for (d_k, d_v) in d_map {
                if let Some(pos) = merged.iter().position(|(c_k, _)| c_k == d_k) {
                    let c_v = &merged[pos].1;
                    merged[pos].1 = merge_values_inner(d_v, c_v);
                } else {
                    merged.push((d_k.clone(), d_v.clone()));
                }
            }
            Value::Map(merged)
        }
        (Value::Seq(d_seq), Value::Seq(c_seq)) => {
            let mut merged = c_seq.clone();
            for item in d_seq {
                if !merged.contains(item) {
                    merged.push(item.clone());
                }
            }
            Value::Seq(merged)
        }
        (direct_val, _) => direct_val.clone(),
    }
}
