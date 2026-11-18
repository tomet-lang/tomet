//! Methods: what a value of a given type can be asked to do.
//!
//! A method works on its receiver and the arguments it is given, and never
//! converts either: `.join` joins scalars or one-paragraph fragments,
//! `.contains` asks a tuple or a string the question that suits it, and a type
//! with no such method says so.

use std::cmp::Ordering;

use tomet_ast::{Tuple, TupleEntry};
use tomet_scalar::Scalar;
use tomet_span::Span;

use tomet_expr::{Closure, EvalContext, EvalError, EvalValue};

use super::args::Args;
use super::ops::{compare, values_equal};

/// Calls `receiver.member(args)`.
pub(crate) fn call_method(
    receiver: EvalValue,
    member: &str,
    args: Args,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    match (receiver, member) {
        (EvalValue::Tuple(tuple), "filter") => {
            let f = function_arg(member, args)?;
            let items: Vec<EvalValue> = tuple.unnamed().cloned().collect();
            let kept = tomet_query::filter_items(&items, |item| {
                match super::apply(&f, vec![item.clone()], cx)? {
                    EvalValue::Scalar(Scalar::Bool(true)) => Ok(true),
                    EvalValue::Scalar(Scalar::Bool(false)) => Ok(false),
                    other => Err(EvalError::mismatch(
                        "the function given to `.filter`",
                        "a bool",
                        other.type_name(),
                    )),
                }
            })?;
            Ok(EvalValue::Tuple(Tuple::from_unnamed(kept)))
        }
        (EvalValue::Tuple(tuple), "map") => {
            let f = function_arg(member, args)?;
            let mut mapped = Vec::new();
            for item in tuple.unnamed() {
                mapped.push(TupleEntry::unnamed(super::apply(
                    &f,
                    vec![item.clone()],
                    cx,
                )?));
            }
            Tuple::try_from_entries(mapped)
                .map(EvalValue::Tuple)
                .map_err(|e| EvalError::message(e.to_string()))
        }
        (EvalValue::Tuple(tuple), "sort") => {
            sort(&tuple.unnamed().cloned().collect::<Vec<_>>(), args, cx)
        }
        (EvalValue::Tuple(tuple), "join") => {
            let [separator] = take::<1>(member, args)?;
            let EvalValue::Scalar(Scalar::Text(separator)) = separator else {
                return Err(EvalError::mismatch(
                    "`.join`",
                    "a string separator",
                    separator.type_name(),
                ));
            };
            join_tuple(tuple, separator)
        }
        (EvalValue::Tuple(tuple), "contains") => {
            let [needle] = take::<1>(member, args)?;
            Ok(EvalValue::Scalar(Scalar::Bool(
                tuple.unnamed().any(|i| values_equal(i, &needle)),
            )))
        }
        (EvalValue::Tuple(tuple), "has") => {
            let [key] = take::<1>(member, args)?;
            let key_str = match &key {
                EvalValue::Scalar(Scalar::Text(s)) => s.clone(),
                EvalValue::Scalar(Scalar::Int(i)) => i.to_string(),
                _ => {
                    return Err(EvalError::mismatch(
                        "`.has` on a tuple",
                        "a string or int key",
                        key.type_name(),
                    ));
                }
            };
            Ok(EvalValue::Scalar(Scalar::Bool(
                tuple.index_of(&key_str).is_some(),
            )))
        }
        (EvalValue::Tuple(tuple), "len") => {
            take::<0>(member, args)?;
            let count = tuple.unnamed().count();
            Ok(EvalValue::Scalar(Scalar::Int(count as i64)))
        }
        (EvalValue::Tuple(tuple), "item") => {
            let [index] = take::<1>(member, args)?;
            let i = match &index {
                EvalValue::Scalar(Scalar::Int(i)) => *i,
                EvalValue::Scalar(Scalar::Number(text)) => text.parse::<i64>().map_err(|_| {
                    EvalError::mismatch("`item`", "an integer index", index.type_name())
                })?,
                _ => {
                    return Err(EvalError::mismatch(
                        "`item`",
                        "an integer index",
                        index.type_name(),
                    ));
                }
            };
            usize::try_from(i)
                .ok()
                .and_then(|idx| tuple.entries().nth(idx))
                .map(|e| e.value().clone())
                .ok_or_else(|| EvalError::message(format!("`item({i})` is out of range")))
        }
        (EvalValue::Tuple(tuple), "get") => {
            let [key, default] = take::<2>(member, args)?;
            let key_str = match &key {
                EvalValue::Scalar(Scalar::Text(s)) => s.clone(),
                EvalValue::Scalar(Scalar::Int(i)) => i.to_string(),
                _ => {
                    return Err(EvalError::mismatch(
                        "`.get`",
                        "a string or int key",
                        key.type_name(),
                    ));
                }
            };
            Ok(tuple.get(&key_str).cloned().unwrap_or(default))
        }
        (EvalValue::Tuple(tuple), "positional") => {
            take::<0>(member, args)?;
            let entries = tuple
                .unnamed()
                .cloned()
                .map(TupleEntry::unnamed)
                .collect::<Vec<_>>();
            Tuple::try_from_entries(entries)
                .map(EvalValue::Tuple)
                .map_err(|e| EvalError::message(e.to_string()))
        }
        (EvalValue::Tuple(tuple), "named") => {
            take::<0>(member, args)?;
            let entries = tuple
                .entries()
                .filter_map(|e| {
                    e.name()
                        .map(|n| TupleEntry::named(n.to_string(), e.value().clone()))
                })
                .collect::<Vec<_>>();
            Tuple::try_from_entries(entries)
                .map(EvalValue::Tuple)
                .map_err(|e| EvalError::message(e.to_string()))
        }
        (EvalValue::DateTime(value), "format") => {
            let [format] = take::<1>(member, args)?;
            let EvalValue::Scalar(Scalar::Text(format)) = format else {
                return Err(EvalError::mismatch(
                    "`.format`",
                    "a format string",
                    format.type_name(),
                ));
            };
            Ok(EvalValue::Scalar(Scalar::Text(
                tomet_expr::format_datetime(&value, &format),
            )))
        }
        (EvalValue::DateTime(value), "at") => {
            let [offset] = take::<1>(member, args)?;
            let EvalValue::Scalar(Scalar::Text(offset)) = offset else {
                return Err(EvalError::mismatch(
                    "`.at`",
                    "an offset string such as \"+09:00\"",
                    offset.type_name(),
                ));
            };
            let offset = tomet_expr::parse_offset(&offset).ok_or_else(|| {
                EvalError::message(format!(
                    "`.at` wants an offset such as \"+09:00\" or \"Z\", not `{offset}`"
                ))
            })?;
            Ok(EvalValue::DateTime(value.to_offset(offset)))
        }
        (EvalValue::Scalar(Scalar::Text(text)), "contains") => {
            let [needle] = take::<1>(member, args)?;
            let EvalValue::Scalar(Scalar::Text(needle)) = needle else {
                return Err(EvalError::mismatch(
                    "`.contains` on a string",
                    "a string",
                    needle.type_name(),
                ));
            };
            Ok(EvalValue::Scalar(Scalar::Bool(
                text.contains(needle.as_str()),
            )))
        }
        (other, _) => Err(EvalError::UnknownMember {
            on: other.type_name(),
            member: member.to_string(),
        }),
    }
}

/// Joins a sequence of scalars into a single text scalar, or a sequence of
/// one-paragraph fragments into a single one-paragraph fragment separated by
/// the given text.
///
/// An empty sequence gives empty text, because a fragment of no blocks cannot
/// stand in the inline position.
fn join_tuple(tuple: Tuple<EvalValue>, separator: String) -> Result<EvalValue, EvalError> {
    let mut items = tuple.into_unnamed();
    let Some(first) = items.next() else {
        return Ok(EvalValue::Scalar(Scalar::Text(String::new())));
    };

    if matches!(&first, EvalValue::Fragment(_)) {
        let span = Span::default();
        let mut combined_inlines = Vec::new();
        combined_inlines.extend(extract_single_paragraph(first, 0)?);

        for (offset, item) in items.enumerate() {
            let index = offset + 1;
            let inlines = extract_single_paragraph(item, index)?;
            combined_inlines.push(tomet_ast::Inline::Text(tomet_ast::Text::new(
                separator.clone(),
                span,
            )));
            combined_inlines.extend(inlines);
        }

        Ok(EvalValue::Fragment(vec![tomet_ast::Block::Paragraph(
            tomet_ast::Paragraph::new(combined_inlines, span),
        )]))
    } else {
        let mut parts = Vec::new();
        parts.push(scalar_text(&first).map_err(|e| wrap_join_scalar_error(e, 0))?);

        for (offset, item) in items.enumerate() {
            let index = offset + 1;
            parts.push(scalar_text(&item).map_err(|e| wrap_join_scalar_error(e, index))?);
        }

        Ok(EvalValue::Scalar(Scalar::Text(parts.join(&separator))))
    }
}

fn wrap_join_scalar_error(err: EvalError, index: usize) -> EvalError {
    match err {
        EvalError::TypeMismatch { expected, got, .. } => EvalError::TypeMismatch {
            what: format!("element {index} of `.join`"),
            expected,
            got,
        },
        other => other,
    }
}

fn extract_single_paragraph(
    val: EvalValue,
    index: usize,
) -> Result<Vec<tomet_ast::Inline>, EvalError> {
    match val {
        EvalValue::Fragment(blocks) => match <[tomet_ast::Block; 1]>::try_from(blocks) {
            Ok([tomet_ast::Block::Paragraph(p)]) => Ok(p.content),
            Ok([_non_paragraph]) => Err(EvalError::mismatch(
                format!("element {index} of `.join`"),
                "a one-paragraph fragment",
                "non-paragraph block",
            )),
            Err(blocks) if blocks.is_empty() => Err(EvalError::mismatch(
                format!("element {index} of `.join`"),
                "a one-paragraph fragment",
                "fragment of zero blocks",
            )),
            Err(_) => Err(EvalError::mismatch(
                format!("element {index} of `.join`"),
                "a one-paragraph fragment",
                "fragment of several blocks",
            )),
        },
        other => Err(EvalError::mismatch(
            format!("element {index} of `.join`"),
            "a one-paragraph fragment",
            other.type_name(),
        )),
    }
}

/// The text of a scalar, for `.join` and macro expansion. Only scalars (other than `none`) have one.
pub(crate) fn scalar_text(value: &EvalValue) -> Result<String, EvalError> {
    if matches!(value, EvalValue::Scalar(Scalar::None)) {
        return Err(EvalError::mismatch(
            "turning a value into text",
            "a string, number or bool",
            value.type_name(),
        ));
    }
    value.scalar_text().ok_or_else(|| {
        EvalError::mismatch(
            "turning a value into text",
            "a string, number or bool",
            value.type_name(),
        )
    })
}

/// `item(n)`: zero-based, no negative index.
pub(crate) fn index_into(items: &[EvalValue], index: &EvalValue) -> Result<EvalValue, EvalError> {
    let i = match index {
        EvalValue::Scalar(Scalar::Int(i)) => *i,
        EvalValue::Scalar(Scalar::Number(text)) => text
            .parse::<i64>()
            .map_err(|_| EvalError::mismatch("`item`", "an integer index", index.type_name()))?,
        _ => {
            return Err(EvalError::mismatch(
                "`item`",
                "an integer index",
                index.type_name(),
            ));
        }
    };
    usize::try_from(i)
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
        EvalValue::Scalar(Scalar::Int(_) | Scalar::Float(_) | Scalar::Number(_)) => {
            // A NaN is a number with no place in an order.
            compare(key, key)?;
            Ok(0)
        }
        EvalValue::Scalar(Scalar::Text(_)) => Ok(1),
        EvalValue::DateTime(_) => Ok(2),
        EvalValue::Version(_) => Ok(3),
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
        Some(EvalValue::Scalar(Scalar::Text(order))) if order == "asc" => false,
        Some(EvalValue::Scalar(Scalar::Text(order))) if order == "desc" => true,
        Some(_) => {
            return Err(EvalError::message(
                "the order given to `.sort` must be \"asc\" or \"desc\"",
            ));
        }
    };

    let mut first_class: Option<(EvalValue, u8)> = None;
    let mut keys = Vec::with_capacity(items.len());
    for item in items {
        match super::apply(&f, vec![item.clone()], cx)? {
            EvalValue::Scalar(Scalar::None) => keys.push(None),
            key => {
                let class = key_class(&key)?;
                match &first_class {
                    Some((first, expected)) if class != *expected => {
                        return Err(EvalError::NotComparable {
                            left: first.type_name(),
                            right: key.type_name(),
                        });
                    }
                    None => first_class = Some((key.clone(), class)),
                    _ => {}
                }
                keys.push(Some(key));
            }
        }
    }
    let mut key_iter = keys.into_iter();
    let sorted = tomet_query::sort_items_by_key(
        items,
        descending,
        |_| Ok(key_iter.next().flatten()),
        |a, b| compare(a, b).unwrap_or(Ordering::Equal),
    )?;
    Ok(EvalValue::Tuple(Tuple::from_unnamed(sorted)))
}
