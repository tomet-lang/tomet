//! Type admission and conformance checking for AST [`Value`]s.

use tomet_ast::Value;
use tomet_type::Type;

/// A value that is not of the type it was held to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub ty: Type,
    pub found: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not {}", self.found, self.ty.describe())
    }
}

impl std::error::Error for Mismatch {}

/// Whether `value`, as a document holds it, is of this type.
///
/// A type that is text -- `string`, `version`, `uuid`, `nanoid`, and `datetime`
/// -- takes any scalar as the text it was written as, so a bare `2024` is a
/// `string` and a bare `1.10` is a `version`: what is written decides, not
/// what the reader would have made of it. A number type takes the numbers
/// the reader keeps as written too (`1.10` is a `float`, `-0` an `int`).
pub fn admits(ty: &Type, value: &Value) -> Result<(), Mismatch> {
    let integer = || match value {
        Value::Int(n) => Some(*n),
        Value::Number(text) => text.parse::<i64>().ok(),
        _ => None,
    };
    let fits = match ty {
        Type::Int => integer().is_some(),
        Type::Uint => integer().is_some_and(|n| n >= 0),
        Type::Float => matches!(value, Value::Int(_)) || value.as_number().is_some(),
        Type::Bool => matches!(value, Value::Bool(_)),
        Type::List(inner) => match value {
            Value::Seq(items) => {
                if let Some(inner_ty) = inner {
                    for item in items {
                        admits(inner_ty, item)?;
                    }
                }
                true
            }
            _ => false,
        },
        Type::String | Type::Nanoid => value.scalar_text().is_some(),
        Type::Datetime | Type::Version | Type::Uuid => value
            .scalar_text()
            .is_some_and(|text| ty.holds_text(&text) == Some(true)),
    };
    if fits {
        Ok(())
    } else {
        Err(Mismatch {
            ty: ty.clone(),
            found: describe_value(value),
        })
    }
}

/// A short sentence fragment naming what `value` is, for an error sentence.
fn describe_value(value: &Value) -> String {
    match value {
        Value::String(text) => format!("`{text}`"),
        Value::Bool(b) => format!("`{b}`"),
        Value::Int(n) => format!("`{n}`"),
        Value::Float(n) => format!("`{n}`"),
        Value::Null => "null".to_string(),
        Value::Seq(_) => "a list".to_string(),
        Value::Map(_) => "a map".to_string(),
        Value::Number(text) => format!("`{text}`"),
        Value::Call(name, _) => format!("`{name}(...)`"),
        Value::Interp(_) => "`${...}`".to_string(),
        Value::Hole(_) => "`<?...>`".to_string(),
        Value::Element(_) => "an element".to_string(),
        Value::Blocks(_) => "blocks".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::String(s.to_string())
    }

    #[test]
    fn admits_intended_values() {
        let cases: &[(Type, Value)] = &[
            (Type::String, Value::String("hi".to_string())),
            (Type::String, Value::Int(2024)),
            (Type::String, Value::Bool(true)),
            (Type::String, Value::Number("1.10".to_string())),
            (Type::Nanoid, text("short-id")),
            (Type::Nanoid, Value::Int(12345678)),
            (Type::Int, Value::Int(-3)),
            (Type::Int, Value::Number("-0".to_string())),
            (Type::Float, Value::Number("1.10".to_string())),
            (Type::Uint, Value::Int(0)),
            (Type::Float, Value::Float(1.5)),
            (Type::Float, Value::Int(2)),
            (Type::Bool, Value::Bool(false)),
            (Type::Datetime, text("2026-10-25")),
            (Type::Datetime, text("2026-10-25T14:30:00Z")),
            (Type::Version, text("1.10.0")),
            (Type::Version, text("1.0.0-beta")),
            (Type::Version, Value::Int(1)),
            (Type::Version, Value::Float(1.5)),
            (Type::Version, Value::Number("1.10".to_string())),
            (Type::Uuid, text("123E4567-E89B-12D3-A456-426614174000")),
            (Type::Uuid, text("123e4567-e89b-12d3-a456-426614174000")),
            (Type::List(None), Value::Seq(vec![])),
            (Type::List(None), Value::Seq(vec![text("a")])),
            (
                Type::List(Some(Box::new(Type::Int))),
                Value::Seq(vec![Value::Int(1), Value::Int(2)]),
            ),
        ];
        for (ty, value) in cases {
            assert_eq!(admits(ty, value), Ok(()), "{ty}: {value:?}");
        }

        let bad_cases: &[(Type, Value)] = &[
            (Type::String, Value::Null),
            (Type::String, Value::Seq(vec![])),
            (Type::Int, Value::Float(1.0)),
            (Type::Uint, Value::Int(-1)),
            (Type::Float, Value::Bool(true)),
            (Type::Bool, Value::Int(1)),
            (Type::Datetime, Value::Int(2026)),
            (Type::Datetime, text("not a date")),
            (Type::Version, text("1.0.0_beta")),
            (Type::Version, Value::Bool(true)),
            (Type::Version, Value::Null),
            (Type::Version, Value::Float(1e21)),
            (Type::Uuid, text("not-a-uuid")),
            (Type::Nanoid, Value::Seq(vec![])),
            (Type::Nanoid, Value::Map(vec![])),
            (Type::List(None), Value::Int(3)),
            (
                Type::List(Some(Box::new(Type::Int))),
                Value::Seq(vec![Value::Int(1), Value::String("not an int".into())]),
            ),
        ];
        for (ty, value) in bad_cases {
            assert!(admits(ty, value).is_err(), "{ty}: {value:?}");
        }
    }
}
