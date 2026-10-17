//! Equality, ordering, and arithmetic on evaluated values.

use std::cmp::Ordering;

use super::error::EvalError;
use super::value::EvalValue;

fn number(value: &EvalValue) -> Option<f64> {
    match value {
        EvalValue::Int(i) => Some(*i as f64),
        EvalValue::Float(f) => Some(*f),
        _ => None,
    }
}

/// `==`. Numbers compare as numbers, so `1 == 1.0`. Values of different
/// types are not equal; that is an answer, not an error, which is what lets
/// `d.meta.draft == true` be asked of a document with no `draft`.
pub(crate) fn values_equal(a: &EvalValue, b: &EvalValue) -> bool {
    use EvalValue as V;
    match (a, b) {
        (V::Int(x), V::Int(y)) => x == y,
        _ if number(a).is_some() && number(b).is_some() => number(a) == number(b),
        (V::Null, V::Null) => true,
        (V::Bool(x), V::Bool(y)) => x == y,
        (V::String(x), V::String(y)) => x == y,
        (V::Seq(x), V::Seq(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(l, r)| values_equal(l, r))
        }
        (V::Map(x), V::Map(y)) => {
            x.len() == y.len()
                && x.iter().all(|(key, l)| {
                    y.iter()
                        .find(|(k, _)| k == key)
                        .is_some_and(|(_, r)| values_equal(l, r))
                })
        }
        (V::Call(n1, a1), V::Call(n2, a2)) => {
            n1 == n2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(l, r)| values_equal(l, r))
        }
        (V::DateTime(x), V::DateTime(y)) => x == y,
        (V::Document(x), V::Document(y)) => x == y,
        (V::Directory(x), V::Directory(y)) => x == y,
        (V::Namespace(x), V::Namespace(y)) => x == y,
        (V::Node { doc: d1, node: n1 }, V::Node { doc: d2, node: n2 }) => d1 == d2 && n1 == n2,
        (V::Fragment(x), V::Fragment(y)) => x == y,
        (V::Function(x), V::Function(y)) => x == y,
        _ => false,
    }
}

/// `<`, `<=`, `>`, `>=` and the key order of `.sort`: two numbers, two
/// strings (byte order, so an ISO 8601 date sorts as text), or two datetimes.
/// Anything else has no order.
pub(crate) fn compare(a: &EvalValue, b: &EvalValue) -> Result<Ordering, EvalError> {
    use EvalValue as V;
    let not_comparable = || EvalError::NotComparable {
        left: a.type_name(),
        right: b.type_name(),
    };
    match (a, b) {
        (V::Int(x), V::Int(y)) => Ok(x.cmp(y)),
        (V::String(x), V::String(y)) => Ok(x.cmp(y)),
        (V::DateTime(x), V::DateTime(y)) => Ok(x.cmp(y)),
        _ => match (number(a), number(b)) {
            // `None` only for a NaN, which has no place in an order.
            (Some(x), Some(y)) => x.partial_cmp(&y).ok_or_else(not_comparable),
            _ => Err(not_comparable()),
        },
    }
}

#[derive(Clone, Copy)]
enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    fn of(function: &str, value: &EvalValue) -> Result<Number, EvalError> {
        match value {
            EvalValue::Int(i) => Ok(Number::Int(*i)),
            EvalValue::Float(f) => Ok(Number::Float(*f)),
            other => Err(EvalError::mismatch(
                format!("`{function}`"),
                "a number",
                other.type_name(),
            )),
        }
    }

    fn as_f64(self) -> f64 {
        match self {
            Number::Int(i) => i as f64,
            Number::Float(f) => f,
        }
    }
}

/// `add`, `sub`, `mul`: two integers stay an integer, falling back to a float
/// on overflow; a float on either side gives a float.
pub(crate) fn arithmetic(
    function: &str,
    a: &EvalValue,
    b: &EvalValue,
) -> Result<EvalValue, EvalError> {
    let (x, y) = (Number::of(function, a)?, Number::of(function, b)?);
    let float = |f: fn(f64, f64) -> f64| EvalValue::Float(f(x.as_f64(), y.as_f64()));
    Ok(match (function, x, y) {
        ("add", Number::Int(i), Number::Int(j)) => i
            .checked_add(j)
            .map(EvalValue::Int)
            .unwrap_or_else(|| float(|p, q| p + q)),
        ("sub", Number::Int(i), Number::Int(j)) => i
            .checked_sub(j)
            .map(EvalValue::Int)
            .unwrap_or_else(|| float(|p, q| p - q)),
        ("mul", Number::Int(i), Number::Int(j)) => i
            .checked_mul(j)
            .map(EvalValue::Int)
            .unwrap_or_else(|| float(|p, q| p * q)),
        ("add", ..) => float(|p, q| p + q),
        ("sub", ..) => float(|p, q| p - q),
        ("mul", ..) => float(|p, q| p * q),
        ("div", ..) => {
            if y.as_f64() == 0.0 {
                return Err(EvalError::DivisionByZero);
            }
            // True division even for two integers: `div(7, 2)` is `3.5`.
            float(|p, q| p / q)
        }
        ("mod", Number::Int(i), Number::Int(j)) => {
            if j == 0 {
                return Err(EvalError::DivisionByZero);
            }
            EvalValue::Int(i.wrapping_rem(j))
        }
        ("mod", ..) => {
            if y.as_f64() == 0.0 {
                return Err(EvalError::DivisionByZero);
            }
            float(|p, q| p % q)
        }
        _ => unreachable!("arithmetic is called only for add, sub, mul, div and mod"),
    })
}
