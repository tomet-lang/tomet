//! The initial builtin function set: `add`/`sub`/`mul`/`div`/`mod`, all
//! binary and numeric-only. `add`/`sub`/`mul` stay `Value::Int` when both
//! operands are `Int` (promoting to `Value::Float` only when either
//! operand is, or on `i64` overflow); `div` always produces a `Float`
//! (true division, not integer/floor division -- less surprising for a
//! document-display context than silent truncation); `mod` keeps integer
//! remainder for two `Int`s, otherwise floating-point remainder.

use typedmark_ast::Value;

use crate::error::ComputeError;

pub(crate) fn call(name: &str, args: &[Value]) -> Result<Value, ComputeError> {
    let (a, b) = require_two(name, args)?;
    let a = as_number(name, a)?;
    let b = as_number(name, b)?;
    match name {
        "add" => Ok(checked_or_float(a, b, i64::checked_add, |x, y| x + y)),
        "sub" => Ok(checked_or_float(a, b, i64::checked_sub, |x, y| x - y)),
        "mul" => Ok(checked_or_float(a, b, i64::checked_mul, |x, y| x * y)),
        "div" => div(a, b),
        "mod" => rem(a, b),
        _ => Err(ComputeError::UnknownFunction(name.to_string())),
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

fn as_number(name: &str, value: &Value) -> Result<Number, ComputeError> {
    match value {
        Value::Int(i) => Ok(Number::Int(*i)),
        Value::Float(f) => Ok(Number::Float(*f)),
        other => Err(ComputeError::NotNumeric {
            function: name.to_string(),
            value: other.clone(),
        }),
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
