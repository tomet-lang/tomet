//! =[ motem ]
//!
//! The data language dual to tomet (`tmt` / `mtm`): a lightweight, expressive
//! syntax for values, maps and sequences, and the parser crate for it. A
//! standalone motem file is a top-level map written as bare `key: value` lines.
//!
//! =[ Overview ]
//!
//! motem provides:
//! - Pure syntax parsing for values, maps, sequences (`list(...)`), and expressions into [`Value`].
//! - Serde integration ([`from_str`], [`to_string`]) for serializing and deserializing Rust types.
//! - AST and parser primitives shared with the document-level parser (`tomet-syntax-parser`).
//!
//! =[ Key Functions ]
//!
//! - (from_str) Parse a motem data document into a Rust type via Serde.
//! - (to_string) Render a Rust type into motem data text via Serde.
//! - (from_value) Convert a [`Value`] into a Rust type via Serde deserializer.
//! - (to_value) Convert a Rust type into a [`Value`] via Serde serializer.
//! - (write_key) Write a map key, quoted only when it is not an identifier. Every printer that emits `key: value` calls it.
//! - (float_text) Spell a float so it reads back as a float. Every printer that emits a [`Value::Float`] calls it.
//!
//! =[ Keys ]
//!
//! A map key is a bare identifier (letters, digits, `_`, `-`, `.`) or a quoted
//! string, so a key can be a path or a URL:
//!
//! ```text
//! "tmtroot/schema/rust.vocabulary.tmt": "../vocabularies/rust.vocabulary.tmt"
//! plain.key: 1
//! ```
//!
//! A quoted string is a key only when a `:` follows it; otherwise it is a
//! value. The empty key is the positional-entry marker, so `"": v` is an error.
//!
//! =[ Scalars ]
//!
//! A bare (unquoted) scalar is typed by its text, and only these are not
//! strings:
//!
//! - `null`, `true`, `false`: the only keywords (`NULL`, `~`, `yes` are strings).
//! - An int, `-?(0|[1-9][0-9]*)`, that fits an `i64`.
//! - A float: an int followed by a fraction (`.5` after digits) and/or an
//!   exponent (`e3`, `E-3`), that is finite.
//!
//! Everything else is a string, so reading a bare scalar never changes its
//! text: `007`, `+1`, `.5`, `5.`, `1_000`, `0x10`, `nan` and `inf` are strings.
//! A number that has the shape of an int or float but does not fit (`i64`
//! overflow, `1e999`) is an error, never a silently lossy value.
//!
//! `float(x)` is the literal constructor for a float (`float()` is `0.0`, an
//! int widens, a string is parsed). It is the way to write `float(nan)`,
//! `float(inf)` and `float(-inf)`, which have no bare spelling. The printer
//! writes a float with a `.` or exponent (`1.0`, never `1`), so a float reads
//! back as a float, and a non-finite one as the constructor.
//!
//! Serializing a `u64` above `i64::MAX` is an error rather than a wrapped
//! value, since [`Value::Int`] is an `i64`.

pub mod de;
pub mod error;
pub mod parser;
pub mod print;
pub mod ser;

pub use de::from_value;
pub use error::{Error, Result};
pub use parser::{
    NoHook, POSITIONAL_ENTRY_KEY, ValueHook, eat_ident, eat_name, eat_name_segment, eat_scalar_raw,
    err, is_ident_char, is_inline_ws, is_name_char, is_name_start, is_name_start_at,
    is_scheme_uri_colon, parse_entry_value, parse_entry_value_with, parse_map_body,
    parse_map_body_with, parse_one_entry, parse_one_entry_with, parse_quoted, parse_value,
    parse_value_at, parse_value_at_with, parse_value_with, scalar_from_text, skip_block_comment,
    skip_inline_ws, skip_line_comment, skip_ws_and_newlines, skip_ws_newlines_and_comments,
    starts_absolute_path, try_parse_call, try_parse_call_with,
};
pub use print::{float_text, key_text, print_value, write_float, write_key, write_scalar_string};
pub use ser::to_value;
pub use tomet_ast::Value;

/// Parse a motem data document into `T` via Serde.
pub fn from_str<T: for<'de> serde::Deserialize<'de>>(src: &str) -> Result<T> {
    let value = normalize_list_calls(parse_value(src)?);
    de::from_value(value)
}

/// Render `T` into motem data text via Serde.
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
        assert_eq!(parse_value("2.5").unwrap(), Value::Float(2.5));
        assert_eq!(
            parse_value("\"hello world\"").unwrap(),
            Value::String("hello world".into())
        );
    }

    #[test]
    fn parse_map_and_nested() {
        let src = r#"
            title: "Motem Language"
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
                assert_eq!(entries[0].1, Value::String("Motem Language".into()));
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

    fn map(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
    }

    fn text(s: &str) -> Value {
        Value::String(s.into())
    }

    #[test]
    fn a_quoted_string_before_a_colon_is_a_key() {
        let src = r#"
            "tmtroot/a/b.tmt": "../vocab/b.tmt"
            "with space": 1
            plain.key: 2
        "#;
        assert_eq!(
            parse_value(src).unwrap(),
            map(&[
                ("tmtroot/a/b.tmt", text("../vocab/b.tmt")),
                ("with space", Value::Int(1)),
                ("plain.key", Value::Int(2)),
            ])
        );
    }

    #[test]
    fn a_quoted_key_can_hold_a_nested_map() {
        let src = r#"
            "tmtroot/w.tmt": {
                git: "https://example.com/v.git"
                rev: main
                path: "w.tmt"
            }
        "#;
        assert_eq!(
            parse_value(src).unwrap(),
            map(&[(
                "tmtroot/w.tmt",
                map(&[
                    ("git", text("https://example.com/v.git")),
                    ("rev", text("main")),
                    ("path", text("w.tmt")),
                ])
            )])
        );
    }

    #[test]
    fn a_quoted_string_without_a_colon_stays_a_value() {
        assert_eq!(parse_value("\"hello\"").unwrap(), text("hello"));
        assert_eq!(
            parse_value("\"x\", k: 1").unwrap(),
            map(&[(POSITIONAL_ENTRY_KEY, text("x")), ("k", Value::Int(1))])
        );
    }

    #[test]
    fn a_quoted_key_must_not_be_empty() {
        let err = parse_value("\"\": 1").unwrap_err().to_string();
        assert!(
            err.contains("must not be empty"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn an_unterminated_quoted_key_reports_the_string() {
        let err = parse_value("\"a/b: 1").unwrap_err().to_string();
        assert!(err.contains("unterminated"), "unexpected message: {err}");
    }

    #[test]
    fn keys_are_quoted_only_when_they_need_to_be() {
        let value = map(&[
            ("a.b-c_d", Value::Int(1)),
            ("日誌", Value::Int(2)),
            ("a/b", Value::Int(3)),
            ("with space", Value::Int(4)),
            ("say \"hi\"", Value::Int(5)),
            ("", Value::Int(6)),
        ]);
        assert_eq!(
            print_value(&value),
            "a.b-c_d: 1\n日誌: 2\n\"a/b\": 3\n\"with space\": 4\n\"say \\\"hi\\\"\": 5\n\"\": 6"
        );
        assert_eq!(key_text("plain"), "plain");
        assert_eq!(key_text("a/b"), "\"a/b\"");
    }

    #[test]
    fn keys_that_need_quotes_read_back_as_the_same_keys() {
        let value = map(&[
            ("a/b.tmt", text("x")),
            ("with space", Value::Int(1)),
            ("say \"hi\"", Value::Bool(true)),
            (
                "nested",
                map(&[("c/d", Value::Int(2)), ("e", Value::Int(3))]),
            ),
        ]);
        assert_eq!(parse_value(&print_value(&value)).unwrap(), value);
    }

    #[test]
    fn a_map_keyed_by_paths_round_trips_through_serde() {
        use std::collections::BTreeMap;

        let mut managed = BTreeMap::new();
        managed.insert(
            "tmtroot/schema/rust.vocabulary.tmt".to_string(),
            "../v/rust".to_string(),
        );
        managed.insert(
            "tmtroot/schema/writ.vocabulary.tmt".to_string(),
            "../v/writ".to_string(),
        );

        let text = to_string(&managed).unwrap();
        assert!(
            text.contains("\"tmtroot/schema/rust.vocabulary.tmt\": "),
            "{text}"
        );
        assert_eq!(
            from_str::<BTreeMap<String, String>>(&text).unwrap(),
            managed
        );
    }

    fn scalar(src: &str) -> Value {
        parse_value(src).unwrap()
    }

    #[test]
    fn only_null_true_and_false_are_keywords() {
        assert_eq!(scalar("null"), Value::Null);
        assert_eq!(scalar("true"), Value::Bool(true));
        assert_eq!(scalar("false"), Value::Bool(false));
        for other in ["NULL", "Null", "~", "True", "FALSE", "yes", "on", "no"] {
            assert_eq!(scalar(other), text(other), "{other}");
        }
    }

    #[test]
    fn numbers_follow_the_json_grammar() {
        assert_eq!(scalar("0"), Value::Int(0));
        assert_eq!(scalar("-0"), Value::Int(0));
        assert_eq!(scalar("42"), Value::Int(42));
        assert_eq!(scalar("-17"), Value::Int(-17));
        assert_eq!(scalar("9223372036854775807"), Value::Int(i64::MAX));
        assert_eq!(scalar("-9223372036854775808"), Value::Int(i64::MIN));
        assert_eq!(scalar("1.50"), Value::Float(1.5));
        assert_eq!(scalar("-0.5"), Value::Float(-0.5));
        assert_eq!(scalar("1e3"), Value::Float(1000.0));
        assert_eq!(scalar("1E+3"), Value::Float(1000.0));
        assert_eq!(scalar("25e-1"), Value::Float(2.5));
    }

    #[test]
    fn text_that_is_not_json_number_stays_a_string() {
        for other in [
            "007",
            "+1",
            "-",
            "1_000",
            "0x10",
            "0b1",
            ".5",
            "5.",
            "1e",
            "1e+",
            "1.2.3",
            "nan",
            "NaN",
            "inf",
            "-inf",
            "infinity",
            "2024-01-02",
            "12:30",
            "<b>",
        ] {
            // in a value position: a bare `12:30` at the top is `12: 30`
            assert_eq!(
                parse_value(&format!("k: {other}")).unwrap(),
                map(&[("k", text(other))]),
                "{other}"
            );
        }
    }

    #[test]
    fn a_number_that_does_not_fit_is_an_error_not_a_lossy_value() {
        for src in [
            "9223372036854775808",
            "-9223372036854775809",
            "1e999",
            "-1e999",
        ] {
            let err = parse_value(src).unwrap_err().to_string();
            assert!(err.contains("out of range"), "{src}: {err}");
        }
    }

    #[test]
    fn the_float_constructor_spells_what_a_bare_scalar_cannot() {
        assert_eq!(scalar("float()"), Value::Float(0.0));
        assert_eq!(scalar("float(2)"), Value::Float(2.0));
        assert_eq!(scalar("float(1.5)"), Value::Float(1.5));
        assert_eq!(scalar("float(\"2.5\")"), Value::Float(2.5));
        assert_eq!(scalar("float(inf)"), Value::Float(f64::INFINITY));
        assert_eq!(scalar("float(-inf)"), Value::Float(f64::NEG_INFINITY));
        assert_eq!(scalar("float(infinity)"), Value::Float(f64::INFINITY));
        assert!(matches!(scalar("float(nan)"), Value::Float(f) if f.is_nan()));
    }

    #[test]
    fn the_float_constructor_rejects_what_does_not_convert() {
        for src in [
            "float(1, 2)",
            "float(abc)",
            "float(true)",
            "float(null)",
            "float(list(1))",
            "float(\"1e999\")",
        ] {
            assert!(parse_value(src).is_err(), "{src}");
        }
    }

    #[test]
    fn a_float_prints_so_it_reads_back_as_a_float() {
        for (f, printed) in [
            (1.0, "1.0"),
            (-0.0, "-0.0"),
            (1.5, "1.5"),
            (1e21, "1e21"),
            (1e-7, "1e-7"),
            (0.1 + 0.2, "0.30000000000000004"),
        ] {
            assert_eq!(float_text(f), printed);
            let line = print_value(&map(&[("k", Value::Float(f))]));
            assert_eq!(parse_value(&line).unwrap(), map(&[("k", Value::Float(f))]));
        }
        for f in [f64::MAX, f64::MIN, f64::MIN_POSITIVE, f64::EPSILON] {
            assert_eq!(scalar(&float_text(f)), Value::Float(f));
        }
    }

    #[test]
    fn a_non_finite_float_prints_as_the_constructor() {
        assert_eq!(float_text(f64::INFINITY), "float(inf)");
        assert_eq!(float_text(f64::NEG_INFINITY), "float(-inf)");
        assert_eq!(float_text(f64::NAN), "float(nan)");
        assert_eq!(
            scalar(&float_text(f64::NEG_INFINITY)),
            Value::Float(f64::NEG_INFINITY)
        );
        assert!(matches!(scalar(&float_text(f64::NAN)), Value::Float(f) if f.is_nan()));
    }

    #[test]
    fn an_int_extreme_reads_back_as_an_int() {
        for i in [0, -1, i64::MAX, i64::MIN] {
            let line = print_value(&map(&[("k", Value::Int(i))]));
            assert_eq!(parse_value(&line).unwrap(), map(&[("k", Value::Int(i))]));
        }
    }

    #[test]
    fn a_string_is_quoted_only_when_bare_text_would_read_back_differently() {
        for (s, printed) in [
            // read back as a string, so bare
            ("007", "k: 007"),
            ("+1", "k: +1"),
            ("inf", "k: inf"),
            ("NULL", "k: NULL"),
            ("1_000", "k: 1_000"),
            // would read back as something else (or not at all), so quoted
            ("null", "k: \"null\""),
            ("true", "k: \"true\""),
            ("12", "k: \"12\""),
            ("1.0", "k: \"1.0\""),
            ("1e3", "k: \"1e3\""),
            ("9223372036854775808", "k: \"9223372036854775808\""),
            ("", "k: \"\""),
        ] {
            let value = map(&[("k", text(s))]);
            assert_eq!(print_value(&value), printed, "{s:?}");
            assert_eq!(parse_value(printed).unwrap(), value, "{s:?}");
        }
    }

    #[test]
    fn a_float_field_keeps_its_float_spelling_through_serde() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Sample {
            x: f64,
            n: i64,
        }
        let sample = Sample { x: 1.0, n: 1 };
        let text = to_string(&sample).unwrap();
        assert_eq!(text, "x: 1.0\nn: 1");
        assert_eq!(from_str::<Sample>(&text).unwrap(), sample);
        // an int widens to a float field, as in serde_json
        assert_eq!(from_str::<f64>("1").unwrap(), 1.0);
    }

    #[test]
    fn an_unsigned_integer_beyond_i64_is_an_error_not_a_wrapped_value() {
        assert_eq!(to_string(&(i64::MAX as u64)).unwrap(), i64::MAX.to_string());
        let err = to_string(&u64::MAX).unwrap_err().to_string();
        assert!(err.contains("out of range"), "{err}");
    }
}
