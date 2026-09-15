//! The initial builtin function set: `add`/`sub`/`mul`/`div`/`mod`, all
//! binary and numeric-only. `add`/`sub`/`mul` stay `Value::Int` when both
//! operands are `Int` (promoting to `Value::Float` only when either
//! operand is, or on `i64` overflow); `div` always produces a `Float`
//! (true division, not integer/floor division -- less surprising for a
//! document-display context than silent truncation); `mod` keeps integer
//! remainder for two `Int`s, otherwise floating-point remainder.
//!
//! # Predicates
//!
//! `eq`/`ne`/`gt`/`gte`/`lt`/`lte`/`contains`/`exists`/`and`/`or`/`not`
//! answer yes-or-no questions about a value. They exist for `${filter(...)}`
//! in an `@kind(index)` document, where each candidate file's metadata is
//! put in an [`crate::EvaluationContext`] and the predicate is evaluated
//! once per file -- but nothing here knows that. They take
//! already-resolved `Value`s and do no I/O, which is what lets them live
//! in this layer at all: `tomet-transform` may call
//! [`crate::evaluate_with_context`], and may not reach the filesystem.
//!
//! Three decisions a caller can see:
//!
//! - **Ordering accepts two numbers or two strings, nothing else.** Two
//!   strings compare bytewise, which is the whole reason strings are
//!   allowed: an ISO-8601 date sorts correctly as text, so
//!   `gte(meta.created, "2026-01-01")` works without a date type. A
//!   mixed pair is [`ComputeError::NotComparable`] rather than a silent
//!   `false`, because a predicate that quietly matches nothing is
//!   indistinguishable from one that legitimately found nothing.
//! - **`Int` and `Float` compare as numbers throughout**, so `eq(1, 1.0)`
//!   is true and agrees with `gte(1, 1.0)`. Falling back to `Value`'s
//!   derived `PartialEq` for equality alone would let `eq` and `gte`
//!   disagree about the same pair, which an author has no way to see.
//! - **`and`/`or` do not short-circuit.** Arguments are evaluated by the
//!   caller before dispatch reaches here, so `or(exists(a), div(1, 0))`
//!   still fails on the division. Making it otherwise means `and`/`or`
//!   taking unevaluated expression trees, which is the shape `filter`
//!   itself needs and a separate piece of work.

use std::cmp::Ordering;

use tomet_ast::Value;

use crate::error::ComputeError;

pub(crate) fn call(name: &str, args: &[Value]) -> Result<Value, ComputeError> {
    match name {
        "add" | "sub" | "mul" | "div" | "mod" => {
            let (a, b) = require_two(name, args)?;
            let a = as_number(name, a)?;
            let b = as_number(name, b)?;
            match name {
                "add" => Ok(checked_or_float(a, b, i64::checked_add, |x, y| x + y)),
                "sub" => Ok(checked_or_float(a, b, i64::checked_sub, |x, y| x - y)),
                "mul" => Ok(checked_or_float(a, b, i64::checked_mul, |x, y| x * y)),
                "div" => div(a, b),
                "mod" => rem(a, b),
                _ => unreachable!(),
            }
        }
        "eq" | "ne" => {
            let (a, b) = require_two(name, args)?;
            let equal = values_eq(a, b);
            Ok(Value::Bool(if name == "eq" { equal } else { !equal }))
        }
        "gt" | "gte" | "lt" | "lte" => {
            let (a, b) = require_two(name, args)?;
            let ordering = compare(name, a, b)?;
            Ok(Value::Bool(match name {
                "gt" => ordering == Ordering::Greater,
                "gte" => ordering != Ordering::Less,
                "lt" => ordering == Ordering::Less,
                "lte" => ordering != Ordering::Greater,
                _ => unreachable!(),
            }))
        }
        "contains" => {
            let (haystack, needle) = require_two(name, args)?;
            Ok(Value::Bool(contains(haystack, needle)))
        }
        "exists" => {
            let value = require_one(name, args)?;
            Ok(Value::Bool(!matches!(value, Value::Null)))
        }
        "and" | "or" => {
            if args.is_empty() {
                return Err(ComputeError::WrongArgCount {
                    function: name.to_string(),
                    expected: 1,
                    got: 0,
                });
            }
            Ok(Value::Bool(match name {
                "and" => args.iter().all(truthy),
                "or" => args.iter().any(truthy),
                _ => unreachable!(),
            }))
        }
        "not" => {
            let value = require_one(name, args)?;
            Ok(Value::Bool(!truthy(value)))
        }
        "date" => call_date(args),
        "time" => call_time(args),
        "uuid" => call_uuid(args),
        "unicode" => call_unicode(args),
        "emoji" => call_emoji(args),
        "tm" => call_tm(args),
        "ref" => call_ref(args),
        _ => Err(ComputeError::UnknownFunction(name.to_string())),
    }
}

fn call_uuid(args: &[Value]) -> Result<Value, ComputeError> {
    match args {
        [] => Ok(Value::String(uuid::Uuid::new_v4().to_string())),
        [Value::String(opt)] if opt == "simple" => {
            Ok(Value::String(uuid::Uuid::new_v4().simple().to_string()))
        }
        [Value::String(opt)] if opt == "nil" => Ok(Value::String(uuid::Uuid::nil().to_string())),
        _ => Ok(Value::String(uuid::Uuid::new_v4().to_string())),
    }
}

fn call_time(args: &[Value]) -> Result<Value, ComputeError> {
    let now = time::OffsetDateTime::now_utc().time();
    match args {
        [] => Ok(Value::String(format!(
            "{:02}:{:02}:{:02}",
            now.hour(),
            now.minute(),
            now.second()
        ))),
        [Value::String(fmt)] => {
            let formatted = fmt
                .replace("HH", &format!("{:02}", now.hour()))
                .replace("MM", &format!("{:02}", now.minute()))
                .replace("SS", &format!("{:02}", now.second()));
            Ok(Value::String(formatted))
        }
        _ => Ok(Value::String(format!(
            "{:02}:{:02}:{:02}",
            now.hour(),
            now.minute(),
            now.second()
        ))),
    }
}

fn call_date(args: &[Value]) -> Result<Value, ComputeError> {
    let today = time::OffsetDateTime::now_utc().date();
    let (y, m, d) = (
        format!("{:04}", today.year()),
        format!("{:02}", today.month() as u8),
        format!("{:02}", today.day()),
    );
    match args {
        [] => Ok(Value::String(format!("{y}-{m}-{d}"))),
        [Value::String(fmt)]
            if fmt.contains("YYYY")
                || fmt.contains("MM")
                || fmt.contains("DD")
                || fmt.contains("YY")
                || fmt.contains("年")
                || fmt.contains("月")
                || fmt.contains("日") =>
        {
            let formatted = fmt
                .replace("YYYY", &y)
                .replace("YY", if y.len() >= 2 { &y[y.len() - 2..] } else { &y })
                .replace("MM", &m)
                .replace("DD", &d);
            Ok(Value::String(formatted))
        }
        [Value::String(s)] => Ok(Value::String(s.clone())),
        [Value::String(s), Value::String(fmt)] => {
            let s_trimmed = s.trim();
            let parts: Vec<&str> = s_trimmed.split(&['-', '/', '.'][..]).collect();
            if parts.len() == 3 {
                let (py, pm, pd) = (parts[0], parts[1], parts[2]);
                let formatted = fmt
                    .replace("YYYY", py)
                    .replace(
                        "YY",
                        if py.len() >= 2 {
                            &py[py.len() - 2..]
                        } else {
                            py
                        },
                    )
                    .replace("MM", pm)
                    .replace("DD", pd);
                Ok(Value::String(formatted))
            } else {
                Ok(Value::String(s.clone()))
            }
        }
        other => Err(ComputeError::WrongArgCount {
            function: "date".to_string(),
            expected: 1,
            got: other.len(),
        }),
    }
}

fn call_unicode(args: &[Value]) -> Result<Value, ComputeError> {
    let val = match args {
        [v] => v,
        other => {
            return Err(ComputeError::WrongArgCount {
                function: "unicode".to_string(),
                expected: 1,
                got: other.len(),
            });
        }
    };
    let code = match val {
        Value::Int(i) => *i as u32,
        Value::String(s) => {
            let clean = s
                .trim()
                .trim_start_matches("U+")
                .trim_start_matches("u+")
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            u32::from_str_radix(clean, 16).map_err(|_| ComputeError::NotNumeric {
                function: "unicode".to_string(),
                value: val.clone(),
            })?
        }
        _ => {
            return Err(ComputeError::NotNumeric {
                function: "unicode".to_string(),
                value: val.clone(),
            });
        }
    };
    let c = char::from_u32(code).ok_or_else(|| ComputeError::NotNumeric {
        function: "unicode".to_string(),
        value: val.clone(),
    })?;
    Ok(Value::String(c.to_string()))
}

fn call_emoji(args: &[Value]) -> Result<Value, ComputeError> {
    let val = match args {
        [v] => v,
        other => {
            return Err(ComputeError::WrongArgCount {
                function: "emoji".to_string(),
                expected: 1,
                got: other.len(),
            });
        }
    };
    let name = match val {
        Value::String(s) => s.trim().trim_matches(':'),
        _ => {
            return Err(ComputeError::NotNumeric {
                function: "emoji".to_string(),
                value: val.clone(),
            });
        }
    };
    let emoji = match name {
        "sparkles" | "sparkle" => "✨",
        "tada" | "party" => "🎉",
        "check" | "white_check_mark" => "✅",
        "warning" | "warn" => "⚠️",
        "fire" | "flame" => "🔥",
        "rocket" => "🚀",
        "smile" | "grinning" => "😊",
        "heart" => "❤️",
        "star" => "⭐",
        "bulb" | "idea" => "💡",
        "memo" | "pencil" => "📝",
        "gear" | "cog" => "⚙️",
        "link" => "🔗",
        "book" => "📖",
        "pin" | "pushpin" => "📌",
        "wave" => "👋",
        "bug" => "🐛",
        "cross" | "x" => "❌",
        "info" | "information_source" => "ℹ️",
        "eyes" => "👀",
        "clap" => "👏",
        "thumbsup" | "+1" => "👍",
        "thumbsdown" | "-1" => "👎",
        "zap" | "lightning" => "⚡",
        "art" => "🎨",
        "construction" | "wip" => "🚧",
        "lock" => "🔒",
        "unlock" => "🔓",
        "key" => "🔑",
        "mag" | "search" => "🔍",
        "sun" => "☀️",
        "moon" => "🌙",
        "coffee" => "☕",
        other => other,
    };
    Ok(Value::String(emoji.to_string()))
}

fn call_tm(args: &[Value]) -> Result<Value, ComputeError> {
    match args {
        [Value::String(path)] => Ok(Value::String(format!("tm:{path}"))),
        [Value::String(path), Value::String(sec)] => Ok(Value::String(format!("tm:{path}#{sec}"))),
        other => Err(ComputeError::WrongArgCount {
            function: "tm".to_string(),
            expected: 1,
            got: other.len(),
        }),
    }
}

fn call_ref(args: &[Value]) -> Result<Value, ComputeError> {
    match args {
        [Value::String(target)] => Ok(Value::String(format!("ref:{target}"))),
        other => Err(ComputeError::WrongArgCount {
            function: "ref".to_string(),
            expected: 1,
            got: other.len(),
        }),
    }
}

#[derive(Clone, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    fn as_f64(self) -> f64 {
        match self {
            Number::Int(i) => i as f64,
            Number::Float(f) => f,
        }
    }
}

fn require_one<'a>(name: &str, args: &'a [Value]) -> Result<&'a Value, ComputeError> {
    match args {
        [a] => Ok(a),
        other => Err(ComputeError::WrongArgCount {
            function: name.to_string(),
            expected: 1,
            got: other.len(),
        }),
    }
}

fn require_two<'a>(name: &str, args: &'a [Value]) -> Result<(&'a Value, &'a Value), ComputeError> {
    match args {
        [a, b] => Ok((a, b)),
        other => Err(ComputeError::WrongArgCount {
            function: name.to_string(),
            expected: 2,
            got: other.len(),
        }),
    }
}

fn number_of(value: &Value) -> Option<Number> {
    match value {
        Value::Int(i) => Some(Number::Int(*i)),
        Value::Float(f) => Some(Number::Float(*f)),
        _ => None,
    }
}

fn as_number(name: &str, value: &Value) -> Result<Number, ComputeError> {
    number_of(value).ok_or_else(|| ComputeError::NotNumeric {
        function: name.to_string(),
        value: value.clone(),
    })
}

/// Equality, with `Int`/`Float` compared as numbers and everything else
/// left to `Value`'s derived `PartialEq`. See this module's header for
/// why the numeric case is special-cased rather than left to the derive.
fn values_eq(a: &Value, b: &Value) -> bool {
    match (number_of(a), number_of(b)) {
        (Some(x), Some(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// Orders two numbers or two strings. Anything else --- including a
/// number against a string --- has no order and says so.
fn compare(name: &str, a: &Value, b: &Value) -> Result<Ordering, ComputeError> {
    let not_comparable = || ComputeError::NotComparable {
        function: name.to_string(),
        left: a.clone(),
        right: b.clone(),
    };
    if let (Some(x), Some(y)) = (number_of(a), number_of(b)) {
        // `partial_cmp` is `None` only for a NaN operand, which has no
        // place in an ordering either.
        return x
            .as_f64()
            .partial_cmp(&y.as_f64())
            .ok_or_else(not_comparable);
    }
    match (a, b) {
        (Value::String(x), Value::String(y)) => Ok(x.cmp(y)),
        _ => Err(not_comparable()),
    }
}

/// Membership, read from the container's own shape: an element of a
/// `Seq`, a key of a `Map`, a substring of a `String`.
///
/// A container this cannot look inside --- `Null` above all, which is
/// what an absent field resolves to --- is `false` rather than an error.
/// `contains(meta.tags, "rust")` asking about a file with no tags is an
/// ordinary "no", not a mistake worth stopping for.
fn contains(haystack: &Value, needle: &Value) -> bool {
    match (haystack, needle) {
        (Value::Seq(items), _) => items.iter().any(|item| values_eq(item, needle)),
        (Value::Map(entries), Value::String(key)) => entries.iter().any(|(k, _)| k == key),
        (Value::String(s), Value::String(sub)) => s.contains(sub.as_str()),
        _ => false,
    }
}

/// Whether a value counts as yes for `and`/`or`/`not`.
///
/// Empty is false: `Null`, `0`, `""`, `[]`, `{}`. The alternative was to
/// require a `Bool` and reject everything else, which reads stricter than
/// it is --- every predicate here already returns a `Bool`, so the rule
/// would only ever fire on a bare field reference, where "is this set to
/// anything" is exactly what the author meant.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Seq(items) => !items.is_empty(),
        Value::Map(entries) => !entries.is_empty(),
        // A call is a literal, not a container with an "empty" state.
        Value::Call(..) => true,
        // Same reasoning as `Call`: an embedded element is a literal, not
        // an emptiable container.
        Value::Element(_) => true,
    }
}

/// `Int op Int` stays `Int` via `checked`, falling back to `Float` both
/// when either operand already is one and on overflow (rather than
/// panicking/wrapping) -- `int_op`/`float_op` compute the same operation
/// for each case.
fn checked_or_float(
    a: Number,
    b: Number,
    int_op: fn(i64, i64) -> Option<i64>,
    float_op: fn(f64, f64) -> f64,
) -> Value {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => match int_op(x, y) {
            Some(result) => Value::Int(result),
            None => Value::Float(float_op(x as f64, y as f64)),
        },
        (a, b) => Value::Float(float_op(a.as_f64(), b.as_f64())),
    }
}

fn div(a: Number, b: Number) -> Result<Value, ComputeError> {
    let divisor = b.as_f64();
    if divisor == 0.0 {
        return Err(ComputeError::DivisionByZero);
    }
    Ok(Value::Float(a.as_f64() / divisor))
}

fn rem(a: Number, b: Number) -> Result<Value, ComputeError> {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) => {
            if y == 0 {
                return Err(ComputeError::DivisionByZero);
            }
            Ok(Value::Int(x % y))
        }
        (a, b) => {
            let divisor = b.as_f64();
            if divisor == 0.0 {
                return Err(ComputeError::DivisionByZero);
            }
            Ok(Value::Float(a.as_f64() % divisor))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(v: Value) -> bool {
        match v {
            Value::Bool(b) => b,
            other => panic!("expected a Bool, got {other:?}"),
        }
    }

    fn seq(items: &[&str]) -> Value {
        Value::Seq(items.iter().map(|s| Value::String(s.to_string())).collect())
    }

    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }

    #[test]
    fn eq_compares_int_and_float_as_numbers() {
        assert!(b(call("eq", &[Value::Int(1), Value::Float(1.0)]).unwrap()));
        assert!(!b(call("ne", &[Value::Int(1), Value::Float(1.0)]).unwrap()));
        // ...and agrees with the orderings about the same pair, which is
        // the reason it does not defer to `Value`'s derived `PartialEq`.
        assert!(b(call("gte", &[Value::Int(1), Value::Float(1.0)]).unwrap()));
        assert!(b(call("lte", &[Value::Int(1), Value::Float(1.0)]).unwrap()));
    }

    #[test]
    fn eq_falls_back_to_structural_equality() {
        assert!(b(call("eq", &[s("rust"), s("rust")]).unwrap()));
        assert!(b(call("ne", &[s("rust"), s("cli")]).unwrap()));
        assert!(b(call("eq", &[seq(&["a", "b"]), seq(&["a", "b"])]).unwrap()));
        assert!(b(call("eq", &[Value::Null, Value::Null]).unwrap()));
    }

    #[test]
    fn iso_dates_order_as_strings() {
        assert!(b(call("gt", &[s("2026-09-11"), s("2026-01-01")]).unwrap()));
        assert!(b(call("lt", &[s("2025-12-31"), s("2026-01-01")]).unwrap()));
        assert!(b(call("gte", &[s("2026-01-01"), s("2026-01-01")]).unwrap()));
        assert!(!b(call("gt", &[s("2026-01-01"), s("2026-01-01")]).unwrap()));
    }

    #[test]
    fn ordering_a_number_against_a_string_is_an_error() {
        let err = call("gt", &[s("a"), Value::Int(1)]).unwrap_err();
        assert!(
            matches!(err, ComputeError::NotComparable { .. }),
            "got {err:?}"
        );
        // Not a silent `false`: a predicate that quietly matches nothing
        // looks exactly like one that found nothing.
    }

    #[test]
    fn contains_reads_the_container_shape() {
        assert!(b(
            call("contains", &[seq(&["rust", "cli"]), s("rust")]).unwrap()
        ));
        assert!(!b(
            call("contains", &[seq(&["rust", "cli"]), s("go")]).unwrap()
        ));
        assert!(b(call("contains", &[s("readme.tmt"), s(".tmt")]).unwrap()));
        let map = Value::Map(vec![("title".to_string(), s("Index"))]);
        assert!(b(call("contains", &[map.clone(), s("title")]).unwrap()));
        assert!(!b(call("contains", &[map, s("author")]).unwrap()));
    }

    #[test]
    fn contains_on_an_absent_field_is_no_rather_than_an_error() {
        // What a file with no `tags:` at all resolves to.
        assert!(!b(call("contains", &[Value::Null, s("rust")]).unwrap()));
    }

    #[test]
    fn exists_is_null_and_nothing_else() {
        assert!(!b(call("exists", &[Value::Null]).unwrap()));
        assert!(b(call("exists", &[s("")]).unwrap()));
        assert!(b(call("exists", &[Value::Seq(vec![])]).unwrap()));
        assert!(b(call("exists", &[Value::Bool(false)]).unwrap()));
    }

    #[test]
    fn and_or_not_are_variadic_over_truthiness() {
        let t = Value::Bool(true);
        let f = Value::Bool(false);
        assert!(b(call("and", &[t.clone(), t.clone(), t.clone()]).unwrap()));
        assert!(!b(call("and", &[t.clone(), f.clone()]).unwrap()));
        assert!(b(call("or", &[f.clone(), f.clone(), t.clone()]).unwrap()));
        assert!(!b(call("or", &[f.clone(), f.clone()]).unwrap()));
        assert!(b(call("not", &[f]).unwrap()));
        assert!(!b(call("not", &[t]).unwrap()));
        // A bare field reference stands in for "is this set to anything".
        assert!(!b(call("and", &[seq(&[]), Value::Bool(true)]).unwrap()));
        assert!(b(call("or", &[Value::Null, s("x")]).unwrap()));
    }

    #[test]
    fn arity_is_checked() {
        for (name, args) in [
            ("eq", vec![Value::Int(1)]),
            ("gt", vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            ("contains", vec![]),
            ("exists", vec![Value::Int(1), Value::Int(2)]),
            ("not", vec![]),
            ("and", vec![]),
            ("or", vec![]),
        ] {
            let err = call(name, &args).unwrap_err();
            assert!(
                matches!(err, ComputeError::WrongArgCount { .. }),
                "`{name}` accepted {} argument(s): {err:?}",
                args.len()
            );
        }
    }

    #[test]
    fn arithmetic_still_rejects_a_non_number() {
        let err = call("add", &[s("a"), Value::Int(1)]).unwrap_err();
        assert!(
            matches!(err, ComputeError::NotNumeric { .. }),
            "got {err:?}"
        );
    }
}
