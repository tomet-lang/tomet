//! Methods: what a value of a given type can be asked to do.
//!
//! A method works on its receiver and the arguments it is given, and never
//! converts either: `.join` joins scalars, `.contains` asks a list, a map or a
//! string the question that suits it, and a type with no such method says so.

use std::cmp::Ordering;

use super::args::Args;
use super::context::EvalContext;
use super::error::EvalError;
use super::ops::{compare, values_equal};
use super::value::{Closure, EvalValue};

/// Calls `receiver.member(args)`.
pub(crate) fn call_method(
    receiver: EvalValue,
    member: &str,
    args: Args,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    match (&receiver, member) {
        (EvalValue::Seq(items), "filter") => {
            let f = function_arg(member, args)?;
            let mut kept = Vec::new();
            for item in items {
                match super::apply(&f, vec![item.clone()], cx)? {
                    EvalValue::Bool(true) => kept.push(item.clone()),
                    EvalValue::Bool(false) => {}
                    other => {
                        return Err(EvalError::mismatch(
                            "the function given to `.filter`",
                            "a bool",
                            other.type_name(),
                        ));
                    }
                }
            }
            Ok(EvalValue::Seq(kept))
        }
        (EvalValue::Seq(items), "map") => {
            let f = function_arg(member, args)?;
            items
                .iter()
                .map(|item| super::apply(&f, vec![item.clone()], cx))
                .collect::<Result<_, _>>()
                .map(EvalValue::Seq)
        }
        (EvalValue::Seq(items), "sort") => sort(items, args, cx),
        (EvalValue::Seq(items), "join") => {
            let [separator] = take::<1>(member, args)?;
            let EvalValue::String(separator) = separator else {
                return Err(EvalError::mismatch(
                    "`.join`",
                    "a string separator",
                    separator.type_name(),
                ));
            };
            let parts = items
                .iter()
                .map(scalar_text)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(EvalValue::String(parts.join(&separator)))
        }
        (EvalValue::Seq(items), "contains") => {
            let [needle] = take::<1>(member, args)?;
            Ok(EvalValue::Bool(
                items.iter().any(|i| values_equal(i, &needle)),
            ))
        }
        (EvalValue::Seq(items), "len") => {
            take::<0>(member, args)?;
            Ok(EvalValue::Int(items.len() as i64))
        }
        (EvalValue::Seq(items), "item") => {
            let [index] = take::<1>(member, args)?;
            index_into(items, &index)
        }
        (EvalValue::DateTime(value), "format") => {
            let [format] = take::<1>(member, args)?;
            let EvalValue::String(format) = format else {
                return Err(EvalError::mismatch(
                    "`.format`",
                    "a format string",
                    format.type_name(),
                ));
            };
            Ok(EvalValue::String(super::datetime::format_datetime(
                value, &format,
            )))
        }
        (EvalValue::DateTime(value), "at") => {
            let [offset] = take::<1>(member, args)?;
            let EvalValue::String(offset) = offset else {
                return Err(EvalError::mismatch(
                    "`.at`",
                    "an offset string such as \"+09:00\"",
                    offset.type_name(),
                ));
            };
            let offset = super::datetime::parse_offset(&offset).ok_or_else(|| {
                EvalError::message(format!(
                    "`.at` wants an offset such as \"+09:00\" or \"Z\", not `{offset}`"
                ))
            })?;
            Ok(EvalValue::DateTime(value.to_offset(offset)))
        }
        (EvalValue::Map(entries), "get") => {
            let [key, default] = take::<2>(member, args)?;
            let EvalValue::String(key) = key else {
                return Err(EvalError::mismatch(
                    "`.get`",
                    "a string key",
                    key.type_name(),
                ));
            };
            Ok(entries
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(default))
        }
        (EvalValue::Map(entries), "contains") => {
            let [key] = take::<1>(member, args)?;
            let EvalValue::String(key) = key else {
                return Err(EvalError::mismatch(
                    "`.contains` on a map",
                    "a string key",
                    key.type_name(),
                ));
            };
            Ok(EvalValue::Bool(entries.iter().any(|(k, _)| *k == key)))
        }
        (EvalValue::String(text), "contains") => {
            let [needle] = take::<1>(member, args)?;
            let EvalValue::String(needle) = needle else {
                return Err(EvalError::mismatch(
                    "`.contains` on a string",
                    "a string",
                    needle.type_name(),
                ));
            };
            Ok(EvalValue::Bool(text.contains(needle.as_str())))
        }
        _ => Err(EvalError::UnknownMember {
            on: receiver.type_name(),
            member: member.to_string(),
        }),
    }
}

/// The text of a scalar, for `.join`. Only scalars have one.
pub(crate) fn scalar_text(value: &EvalValue) -> Result<String, EvalError> {
    match value {
        EvalValue::String(s) => Ok(s.clone()),
        EvalValue::Int(i) => Ok(i.to_string()),
        EvalValue::Float(f) => Ok(f.to_string()),
        EvalValue::Bool(b) => Ok(b.to_string()),
        other => Err(EvalError::mismatch(
            "turning a value into text",
            "a string, number or bool",
            other.type_name(),
        )),
    }
}

/// `item(n)`: zero-based, no negative index.
pub(crate) fn index_into(items: &[EvalValue], index: &EvalValue) -> Result<EvalValue, EvalError> {
    let EvalValue::Int(i) = index else {
        return Err(EvalError::mismatch(
            "`item`",
            "an integer index",
            index.type_name(),
        ));
    };
    usize::try_from(*i)
        .ok()
        .and_then(|i| items.get(i))
        .cloned()
        .ok_or_else(|| EvalError::message(format!("`item({i})` is out of range")))
}

fn take<const N: usize>(member: &str, args: Args) -> Result<[EvalValue; N], EvalError> {
    let values = args.exactly(&format!(".{member}"), N)?;
    Ok(values
        .try_into()
        .unwrap_or_else(|_| unreachable!("`exactly` returned {N} values")))
}

fn function_arg(member: &str, args: Args) -> Result<Closure, EvalError> {
    let [f] = take::<1>(member, args)?;
    match f {
        EvalValue::Function(closure) => Ok(closure),
        other => Err(EvalError::mismatch(
            format!("`.{member}`"),
            "a function",
            other.type_name(),
        )),
    }
}

/// How alike two sort keys must be to be put in order at all.
fn key_class(key: &EvalValue) -> Result<u8, EvalError> {
    match key {
        EvalValue::Int(_) | EvalValue::Float(_) => {
            // A NaN is a number with no place in an order.
            compare(key, key)?;
            Ok(0)
        }
        EvalValue::String(_) => Ok(1),
        EvalValue::DateTime(_) => Ok(2),
        other => Err(EvalError::NotComparable {
            left: other.type_name(),
            right: other.type_name(),
        }),
    }
}

fn sort(items: &[EvalValue], args: Args, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let mut positional = args.between(".sort", 1, 2)?.into_iter();
    let f = match positional.next() {
        Some(EvalValue::Function(closure)) => closure,
        Some(other) => {
            return Err(EvalError::mismatch(
                "`.sort`",
                "a function",
                other.type_name(),
            ));
        }
        None => unreachable!("`exactly_or_between` returned at least one value"),
    };
    let descending = match positional.next() {
        None => false,
        Some(EvalValue::String(order)) if order == "asc" => false,
        Some(EvalValue::String(order)) if order == "desc" => true,
        Some(_) => {
            return Err(EvalError::message(
                "the order given to `.sort` must be \"asc\" or \"desc\"",
            ));
        }
    };

    // An item whose key is `Null` -- a field the document does not have --
    // goes after all the others, whichever way the rest are ordered, and
    // keeps its place among the other `Null`s. Only keys that exist have to
    // agree on a kind.
    let mut keyed = Vec::with_capacity(items.len());
    let mut absent = Vec::new();
    for item in items {
        match super::apply(&f, vec![item.clone()], cx)? {
            EvalValue::Null => absent.push(item.clone()),
            key => keyed.push((key, item.clone())),
        }
    }
    // Check every key before sorting, so the comparison below is a total
    // order and cannot fail half way.
    if let Some((first, _)) = keyed.first() {
        let class = key_class(first)?;
        for (key, _) in &keyed {
            if key_class(key)? != class {
                return Err(EvalError::NotComparable {
                    left: first.type_name(),
                    right: key.type_name(),
                });
            }
        }
    }
    keyed.sort_by(|(a, _), (b, _)| {
        let order = compare(a, b).unwrap_or(Ordering::Equal);
        if descending { order.reverse() } else { order }
    });
    let mut sorted: Vec<EvalValue> = keyed.into_iter().map(|(_, item)| item).collect();
    sorted.extend(absent);
    Ok(EvalValue::Seq(sorted))
}
