//! # TOVE (Tomet Object & Value Expression)
//!
//! A lightweight, expressive data language and parser crate for Tomet.
//!
//! TOVE provides:
//! - Pure syntax parsing for values, maps, sequences (`list(...)`), and expressions into [`Value`].
//! - Serde integration ([`from_str`], [`to_string`]) for serializing and deserializing Rust types.
//! - AST and parser primitives shared with the document-level parser (`tomet-syntax-parser`).

pub mod de;
pub mod error;
pub mod parser;
pub mod print;
pub mod ser;

pub use de::from_value;
pub use error::{Error, Result};
pub use parser::{
    NoHook, POSITIONAL_ENTRY_KEY, ValueHook, eat_ident, eat_name, eat_name_segment,
    eat_scalar_raw, err, is_ident_char, is_inline_ws, is_name_char, is_name_start,
    is_name_start_at, is_scheme_uri_colon, parse_entry_value, parse_entry_value_with,
    parse_map_body, parse_map_body_with, parse_one_entry, parse_one_entry_with, parse_quoted,
    parse_value, parse_value_at, parse_value_at_with, parse_value_with, scalar_from_text,
    skip_block_comment, skip_inline_ws, skip_line_comment, skip_ws_and_newlines,
    skip_ws_newlines_and_comments, starts_absolute_path, try_parse_call, try_parse_call_with,
};
pub use print::{print_value, write_scalar_string};
pub use ser::to_value;
pub use tomet_ast::Value;

/// Parse a TOVE data document into `T` via Serde.
pub fn from_str<T: for<'de> serde::Deserialize<'de>>(src: &str) -> Result<T> {
    let value = normalize_list_calls(parse_value(src)?);
    de::from_value(value)
}

/// Render `T` into TOVE data text via Serde.
pub fn to_string<T: serde::Serialize + ?Sized>(value: &T) -> Result<String> {
    let v = ser::to_value(value)?;
    Ok(print::print_value(&v))
}

/// Normalizes `list(...)` calls into `Value::Seq` so Serde sequence deserialization
/// works seamlessly.
pub fn normalize_list_calls(value: Value) -> Value {
    match value {
        Value::Call(name, args) if name == "list" => {
            Value::Seq(args.into_iter().map(normalize_list_calls).collect())
        }
        Value::Call(name, args) => {
            Value::Call(name, args.into_iter().map(normalize_list_calls).collect())
        }
        Value::Seq(items) => Value::Seq(items.into_iter().map(normalize_list_calls).collect()),
        Value::Map(entries) => Value::Map(
            entries
                .into_iter()
                .map(|(k, v)| (k, normalize_list_calls(v)))
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Settings {
        style: Option<String>,
        status_bar_visible: Option<bool>,
        refresh_ms: Option<u32>,
        font_size: Option<f64>,
        bookmarks: Vec<Bookmark>,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Bookmark {
        label: String,
        path: String,
    }

    #[test]
    fn round_trips_a_struct() {
        let settings = Settings {
            style: Some("Fusion".into()),
            status_bar_visible: Some(true),
            refresh_ms: Some(500),
            font_size: Some(10.5),
            bookmarks: vec![
                Bookmark {
                    label: "Inbox".into(),
                    path: "notes/inbox".into(),
                },
                Bookmark {
                    label: "日誌".into(),
                    path: "notes/journal".into(),
                },
            ],
        };

        let text = to_string(&settings).unwrap();
        let round_tripped: Settings = from_str(&text).unwrap();
        assert_eq!(round_tripped, settings);
    }

    #[test]
    fn missing_optional_fields_deserialize_to_none() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Small {
            a: Option<String>,
            b: i32,
        }
        let parsed: Small = from_str("b: 7").unwrap();
        assert_eq!(parsed, Small { a: None, b: 7 });
    }

    #[test]
    fn enum_round_trips_as_unit_or_newtype() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        enum Effect {
            None,
            Blur(u32),
        }
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Wrapper {
            effect: Effect,
        }

        let a = Wrapper {
            effect: Effect::None,
        };
        let b = Wrapper {
            effect: Effect::Blur(4),
        };
        assert_eq!(from_str::<Wrapper>(&to_string(&a).unwrap()).unwrap(), a);
        assert_eq!(from_str::<Wrapper>(&to_string(&b).unwrap()).unwrap(), b);
    }

    #[test]
    fn call_syntax_has_no_serde_equivalent() {
        let err = de::from_value::<i32>(Value::Call(
            "list".into(),
            vec![Value::String("card".into())],
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("call syntax"), "unexpected message: {msg}");
    }

    #[test]
    fn call_syntax_prints_back_as_a_call() {
        let v = Value::Call(
            "list".into(),
            vec![Value::String("card".into()), Value::Int(2)],
        );
        assert_eq!(print::print_value(&v), "list(card, 2)");
    }

    #[test]
    fn parse_scalars() {
        assert_eq!(parse_value("null").unwrap(), Value::Null);
        assert_eq!(parse_value("true").unwrap(), Value::Bool(true));
        assert_eq!(parse_value("false").unwrap(), Value::Bool(false));
        assert_eq!(parse_value("123").unwrap(), Value::Int(123));
        assert_eq!(parse_value("3.14").unwrap(), Value::Float(3.14));
        assert_eq!(
            parse_value("\"hello world\"").unwrap(),
            Value::String("hello world".into())
        );
    }

    #[test]
    fn parse_map_and_nested() {
        let src = r#"
            title: "Tove Language"
            count: 42
            nested: {
                flag: true
            }
        "#;
        let val = parse_value(src).unwrap();
        match val {
            Value::Map(entries) => {
                assert_eq!(entries.len(), 3);
                assert_eq!(entries[0].0, "title");
                assert_eq!(entries[0].1, Value::String("Tove Language".into()));
                assert_eq!(entries[1].0, "count");
                assert_eq!(entries[1].1, Value::Int(42));
                assert_eq!(entries[2].0, "nested");
                match &entries[2].1 {
                    Value::Map(inner) => {
                        assert_eq!(inner[0].0, "flag");
                        assert_eq!(inner[0].1, Value::Bool(true));
                    }
                    other => panic!("expected map, got {other:?}"),
                }
            }
            other => panic!("expected map, got {other:?}"),
        }
    }

    #[test]
    fn parse_list_call() {
        let val = parse_value("list(1, 2, 3)").unwrap();
        assert_eq!(
            val,
            Value::Call(
                "list".into(),
                vec![Value::Int(1), Value::Int(2), Value::Int(3)]
            )
        );
        let normalized = normalize_list_calls(val);
        assert_eq!(
            normalized,
            Value::Seq(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }
}
