//! Type admission and conformance checking for AST [`Value`]s.

use tomet_ast::{Tuple, Value};
use tomet_scalar::Scalar;
use tomet_type::{Datum, Kind, Type};

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
/// A type that is text -- `string`, `version`, `uuid`, `nanoid`, `datetime` and
/// `date` -- takes any scalar as the text it was written as, so a bare `2024` is
/// a `string` and a bare `1.10` is a `version`: what is written decides, not
/// what the reader would have made of it. A number type takes the numbers
/// the reader keeps as written too (`1.10` is a `float`, `-0` an `int`).
///
/// A `list` with a `length` has exactly that many items, and each is held to
/// its item type when it has one. A type's `default` is not looked at: it is
/// what a value is when there is none, not a rule for the ones there are.
pub fn admits(ty: &Type, value: &Value) -> Result<(), Mismatch> {
    let integer = || match value {
        Value::Int(n) => Some(*n),
        Value::Number(text) => text.parse::<i64>().ok(),
        _ => None,
    };
    let fits = match ty.kind() {
        Kind::Int => integer().is_some(),
        Kind::Uint => integer().is_some_and(|n| n >= 0),
        Kind::Float => matches!(value, Value::Int(_)) || value.as_number().is_some(),
        Kind::Bool => matches!(value, Value::Bool(_)),
        Kind::List => match value {
            Value::Tuple(tuple) if tuple.has_no_named() => {
                if let Some(item_type) = ty.item() {
                    for item in tuple.values() {
                        admits(item_type, item)?;
                    }
                }
                if ty.length().is_some_and(|length| length != tuple.len()) {
                    return Err(Mismatch {
                        ty: ty.clone(),
                        found: format!("a list of {} items", tuple.len()),
                    });
                }
                true
            }
            _ => false,
        },
        Kind::String | Kind::Nanoid => value.scalar_text().is_some(),
        Kind::Datetime | Kind::Date | Kind::Version | Kind::Uuid => value
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

/// Whether `value` is no value yet: a `{$ ... }` is evaluated and a `<$...>` is filled before a
/// document made from the one that holds it is a document, so what it will be is not known and
/// nothing is said of it against a type.
pub fn is_placeholder(value: &Value) -> bool {
    matches!(value, Value::Interp(_) | Value::TypeSeat(_))
}

/// The value a [`Datum`] is when a document holds it: text is text, a whole number an integer and
/// `(a, b)` a list.
pub fn datum_value(datum: &Datum) -> Value {
    match datum {
        Datum::Scalar(s) => Value::from(s.clone()),
        Datum::Seq(items) => Value::Tuple(Tuple::from_unnamed(items.iter().map(datum_value))),
    }
}

/// The value `ty` stands for when none is written, as a document holds it: its `default` when that
/// is a value written out. `None` for a type with no `default`, and for `default: auto`, which is
/// made up when it is needed and is no value to read here.
pub fn default_value(ty: &Type) -> Option<Value> {
    match ty.default()? {
        Datum::Scalar(Scalar::Auto) => None,
        datum => Some(datum_value(datum)),
    }
}

/// The plain value a data value is, or `None` for one that is more than that
/// (a map, an element, content, etc.).
pub fn value_to_datum(value: &Value) -> Option<Datum> {
    Some(match value {
        Value::None => Datum::Scalar(Scalar::None),
        Value::Auto => Datum::Scalar(Scalar::Auto),
        Value::Bool(b) => Datum::Scalar(Scalar::Bool(*b)),
        Value::Int(n) => Datum::Scalar(Scalar::Int(*n)),
        Value::Float(n) => Datum::Scalar(Scalar::Float(*n)),
        Value::Number(text) => match text.parse::<i64>() {
            Ok(n) => Datum::Scalar(Scalar::Int(n)),
            Err(_) => {
                let s = Scalar::from_bare(text).ok()?;
                Datum::Scalar(s)
            }
        },
        Value::String(text) => Datum::Scalar(Scalar::Text(text.clone())),
        Value::Tuple(tuple) if tuple.has_no_named() => {
            Datum::Seq(tuple.unnamed().map(value_to_datum).collect::<Option<_>>()?)
        }
        _ => return None,
    })
}

/// [`admits`] for the `default` a type is written with, when it is a value written out: whether it
/// is of the type that carries it. A type with no `default`, or with `default: auto`, has
/// nothing to hold to it.
pub fn admits_default(ty: &Type) -> Result<(), Mismatch> {
    match default_value(ty) {
        Some(value) => admits(ty, &value),
        None => Ok(()),
    }
}

/// A short sentence fragment naming what `tuple` is, for an error sentence:
///
/// - no named entry and at least one entry -> `"a list"`
/// - no unnamed entry and at least one entry -> `"a map"`
/// - empty -> `"an empty tuple"`
/// - mixed -> `"a list with named entries"`
pub fn describe_tuple(tuple: &Tuple) -> &'static str {
    if tuple.is_empty() {
        "an empty tuple"
    } else if tuple.has_no_named() {
        "a list"
    } else if tuple.has_no_unnamed() {
        "a map"
    } else {
        "a list with named entries"
    }
}

/// A short sentence fragment naming what `value` is, for an error sentence.
pub fn describe_value(value: &Value) -> String {
    match value {
        Value::String(text) => format!("`{text}`"),
        Value::Bool(b) => format!("`{b}`"),
        Value::Int(n) => format!("`{n}`"),
        Value::Float(n) => format!("`{n}`"),
        Value::None => "none".to_string(),
        Value::Auto => "auto".to_string(),
        Value::Tuple(tuple) => describe_tuple(tuple).to_string(),
        Value::Number(text) => format!("`{text}`"),
        Value::Call(name, _) => format!("`{name}(...)`"),
        Value::Interp(_) => "`{$ ... }`".to_string(),
        Value::Hole(_) => "`<?...>`".to_string(),
        Value::TypeSeat(_) => "`<$...>`".to_string(),
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

    fn seq(items: Vec<Value>) -> Value {
        Value::Tuple(Tuple::from_unnamed(items))
    }

    fn map(entries: Vec<(String, Value)>) -> Value {
        Value::Tuple(Tuple::from_named(entries))
    }

    /// The type written as `source`.
    fn t(source: &str) -> Type {
        source.parse().expect("a type")
    }

    #[test]
    fn admits_intended_values() {
        let cases: &[(Type, Value)] = &[
            (t("string"), Value::String("hi".to_string())),
            (t("string"), Value::Int(2024)),
            (t("string"), Value::Bool(true)),
            (t("string"), Value::Number("1.10".to_string())),
            (t("nanoid"), text("short-id")),
            (t("nanoid"), Value::Int(12345678)),
            (t("int"), Value::Int(-3)),
            (t("int"), Value::Number("-0".to_string())),
            (t("float"), Value::Number("1.10".to_string())),
            (t("uint"), Value::Int(0)),
            (t("float"), Value::Float(1.5)),
            (t("float"), Value::Int(2)),
            (t("bool"), Value::Bool(false)),
            (t("datetime"), text("2026-10-25")),
            (t("datetime"), text("2026-10-25T14:30:00Z")),
            (t("date"), text("2026-10-25")),
            (t("date"), text("2024-02-29")),
            (t("version"), text("1.10.0")),
            (t("version"), text("1.0.0-beta")),
            (t("version"), Value::Int(1)),
            (t("version"), Value::Float(1.5)),
            (t("version"), Value::Number("1.10".to_string())),
            (t("uuid"), text("123E4567-E89B-12D3-A456-426614174000")),
            (t("uuid"), text("123e4567-e89b-12d3-a456-426614174000")),
            (
                t("uuid(prefix: \"u:\")"),
                text("u:123e4567-e89b-12d3-a456-426614174000"),
            ),
            (t("nanoid(length: 4, prefix: \"n-\")"), text("anything")),
            (t("string(default: \"x\")"), Value::Int(2024)),
            (t("list(length: 2)"), seq(vec![text("a"), text("b")])),
            (t("list(int, length: 0)"), seq(vec![])),
            (t("list"), seq(vec![])),
            (t("list"), seq(vec![text("a")])),
            (t("list(int)"), seq(vec![Value::Int(1), Value::Int(2)])),
        ];
        for (ty, value) in cases {
            assert_eq!(admits(ty, value), Ok(()), "{ty}: {value:?}");
        }

        let bad_cases: &[(Type, Value)] = &[
            (t("string"), Value::None),
            (t("string"), seq(vec![])),
            (t("int"), Value::Float(1.0)),
            (t("uint"), Value::Int(-1)),
            (t("float"), Value::Bool(true)),
            (t("bool"), Value::Int(1)),
            (t("datetime"), Value::Int(2026)),
            (t("datetime"), text("not a date")),
            (t("version"), text("1.0.0_beta")),
            (t("version"), Value::Bool(true)),
            (t("version"), Value::None),
            (t("version"), Value::Float(1e21)),
            (t("date"), Value::Int(2026)),
            (t("date"), text("2026-02-29")),
            (t("date"), text("2026-10-25T14:30:00Z")),
            (t("uuid"), text("not-a-uuid")),
            (
                t("uuid(prefix: \"u:\")"),
                text("123e4567-e89b-12d3-a456-426614174000"),
            ),
            (t("list(length: 2)"), seq(vec![text("a")])),
            (t("list(string, length: 1)"), seq(vec![])),
            (t("nanoid"), seq(vec![])),
            (t("nanoid"), map(vec![("k".into(), text("v"))])),
            (t("list"), Value::Int(3)),
            (t("list"), map(vec![("k".into(), text("v"))])),
            (
                t("list(int)"),
                seq(vec![Value::Int(1), Value::String("not an int".into())]),
            ),
        ];
        for (ty, value) in bad_cases {
            assert!(admits(ty, value).is_err(), "{ty}: {value:?}");
        }
    }

    #[test]
    fn a_datum_is_the_value_a_document_holds() {
        let datum = Datum::Seq(vec![
            Datum::Scalar(Scalar::None),
            Datum::Scalar(Scalar::Bool(true)),
            Datum::Scalar(Scalar::Int(3)),
            Datum::Scalar(Scalar::Float(1.5)),
            Datum::Scalar(Scalar::Text("a".to_string())),
            Datum::Seq(vec![]),
        ]);
        assert_eq!(
            datum_value(&datum),
            seq(vec![
                Value::None,
                Value::Bool(true),
                Value::Int(3),
                Value::Float(1.5),
                text("a"),
                seq(vec![]),
            ])
        );
    }

    #[test]
    fn a_written_default_is_held_to_its_own_type() {
        for fits in [
            "int(default: 3)",
            "string(default: \"x\")",
            "list(string, length: 2, default: (\"a\", \"b\"))",
            "date(default: \"2026-11-11\")",
            "uuid(default: auto)",
            "int",
        ] {
            assert_eq!(admits_default(&t(fits)), Ok(()), "{fits}");
        }
        for (misfit, said) in [
            ("int(default: \"three\")", "`three` is not an integer"),
            (
                "list(string, length: 3, default: (\"a\"))",
                "a list of 1 items is not a list of 3 items, each a string",
            ),
            ("list(int, default: (1, \"b\"))", "`b` is not an integer"),
            ("date(default: \"soon\")", "`soon` is not a date"),
        ] {
            let err = admits_default(&t(misfit)).unwrap_err();
            assert!(err.to_string().starts_with(said), "{misfit}: {err}");
        }
    }

    #[test]
    fn only_a_written_default_is_a_value() {
        assert_eq!(default_value(&t("int(default: 3)")), Some(Value::Int(3)));
        assert_eq!(default_value(&t("uuid(default: auto)")), None);
        assert_eq!(default_value(&t("int")), None);
    }

    #[test]
    fn an_expression_and_a_seat_are_placeholders_and_a_value_is_not() {
        let parsed = tomet_ast_parser::parse_file("::meta(a: $(x), b: <$int>, c: 1)\n").unwrap();
        let Some(tuple) = &parsed.header[0].data.args else {
            panic!("a tuple");
        };
        let placeholders: Vec<bool> = tuple.values().map(is_placeholder).collect();
        assert_eq!(placeholders, [true, true, false]);
    }

    #[test]
    fn a_list_of_the_wrong_length_says_how_long_it_is() {
        let err = admits(&t("list(string, length: 3)"), &seq(vec![text("a")])).unwrap_err();
        assert_eq!(
            err.to_string(),
            "a list of 1 items is not a list of 3 items, each a string"
        );
    }
}
