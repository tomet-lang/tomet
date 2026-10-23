//! =[ motem-syntax ]
//!
//! The syntax of motem, the data language dual to tomet (`tmt` / `mtm`): a
//! lightweight, expressive syntax for values, maps and sequences. A standalone
//! motem file is a top-level map written as bare `key: value` lines.
//!
//! Depend on `motem` instead unless you are a crate below it; it re-exports this
//! crate and `serde_motem`.
//!
//! =[ Overview ]
//!
//! motem-syntax provides:
//! - Pure syntax parsing for values, maps, and sequences (`list(...)`) into [`Value`].
//! - The printer for the same syntax ([`print_value`] and the spelling helpers).
//! - AST and parser primitives shared with the document-level parser (`tomet-syntax-parser`).
//!
//! Serde integration lives in `serde_motem`.
//!
//! =[ Key Functions ]
//!
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
//! A document with nothing in it, or only whitespace and comments, is the
//! empty map, so a config file may be empty and a reader fills in every
//! optional field. Only the document may be empty: a value slot (`k:`) still
//! needs a value.
//!
//! A named key may appear once in a map: writing it again is an error at the
//! second one (`duplicate key `a``), whether it is spelled bare or quoted.
//! Positional entries have no key and may repeat.
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
//! The printer writes a float with a `.` or exponent (`1.0`, never `1`), so a
//! float reads back as a float. A non-finite float has no bare spelling and is
//! written with the `float(...)` constructor (see below).
//!
//! Serializing a `u64` above `i64::MAX` is an error rather than a wrapped
//! value, since [`Value::Int`] is an `i64`.
//!
//! =[ Constructors ]
//!
//! `name(arg, ...)` with a name in [`LITERAL_CONSTRUCTORS`] is resolved by the
//! parser to the plain value it stands for, so a reader never sees these as a
//! [`Value::Call`]. Arguments are positional only. They convert as Python's
//! `str`, `int` and `float` do, and more than one argument is an error where
//! the table says at most one.
//!
//! - `list(a, b)` is a sequence ([`Value::Seq`]); `list()` is empty.
//! - `string(x)` is the text of `x`; `string()` is `""`. A bare `x` is taken
//!   as written, not typed, so `string(a)` is `"a"` and `string(1.50)` is
//!   `"1.50"`. A quoted string is itself; a nested constructor's value is
//!   rendered the way it is written; a sequence, map or other call is an error.
//! - `int(x)` is an int: `int()` is `0`, a float truncates toward zero, a
//!   string is read as a decimal `i64` (`int("1.5")` is an error).
//! - `float(x)` is a float: `float()` is `0.0`, an int widens, a string is
//!   parsed. It is the only way to write `float(nan)`, `float(inf)` and
//!   `float(-inf)`.
//!
//! Any other callee (`enum(...)`, a typo) stays a [`Value::Call`] for a later
//! pass to judge; motem does not.

pub mod error;
pub mod parser;
pub mod print;

pub use error::{Error, Result};
pub use parser::{
    LITERAL_CONSTRUCTORS, NoHook, POSITIONAL_ENTRY_KEY, ValueHook, duplicate_key_error, eat_ident,
    eat_name, eat_name_segment, eat_scalar_raw, err, is_ident_char, is_inline_ws, is_name_char,
    is_name_start, is_name_start_at, is_scheme_uri_colon, parse_entry_value,
    parse_entry_value_with, parse_map_body, parse_map_body_with, parse_one_entry,
    parse_one_entry_with, parse_quoted, parse_value, parse_value_at, parse_value_at_with,
    parse_value_with, scalar_from_text, skip_block_comment, skip_inline_ws, skip_line_comment,
    skip_ws_and_newlines, skip_ws_newlines_and_comments, starts_absolute_path, try_parse_call,
    try_parse_call_with,
};
pub use print::{float_text, key_text, print_value, write_float, write_key, write_scalar_string};
pub use tomet_ast::Value;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_syntax_prints_back_as_a_call() {
        let v = Value::Call(
            "enum".into(),
            vec![Value::String("card".into()), Value::Int(2)],
        );
        assert_eq!(print::print_value(&v), "enum(card, 2)");
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
        assert_eq!(
            parse_value("list(1, 2, 3)").unwrap(),
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
    fn a_repeated_named_key_is_an_error_at_the_second_one() {
        let err = parse_value("a: 1\nb: 2\na: 3").unwrap_err();
        assert!(err.to_string().contains("duplicate key `a`"), "{err}");
        assert_eq!(err.line, 3, "the error points at the second `a`");
    }

    #[test]
    fn a_quoted_and_a_bare_spelling_of_one_key_are_a_duplicate() {
        let err = parse_value("\"a\": 1\na: 2").unwrap_err().to_string();
        assert!(err.contains("duplicate key `a`"), "{err}");
    }

    #[test]
    fn a_nested_map_is_checked_on_its_own() {
        let err = parse_value("m: { x: 1, x: 2 }").unwrap_err().to_string();
        assert!(err.contains("duplicate key `x`"), "{err}");
        // the same key in two different maps is fine
        assert!(parse_value("a: { x: 1 }\nb: { x: 2 }").is_ok());
    }

    #[test]
    fn positional_entries_may_repeat() {
        assert_eq!(
            parse_value("\"x\", \"x\", k: 1").unwrap(),
            map(&[
                (POSITIONAL_ENTRY_KEY, text("x")),
                (POSITIONAL_ENTRY_KEY, text("x")),
                ("k", Value::Int(1)),
            ])
        );
    }

    #[test]
    fn a_repeated_key_is_found_in_a_large_map_too() {
        let mut src: String = (0..40).map(|i| format!("k{i}: {i}\n")).collect();
        assert!(parse_value(&src).is_ok());
        src.push_str("k3: 0");
        let err = parse_value(&src).unwrap_err().to_string();
        assert!(err.contains("duplicate key `k3`"), "{err}");
    }

    #[test]
    fn list_is_a_sequence_not_a_call() {
        assert_eq!(scalar("list()"), Value::Seq(vec![]));
        assert_eq!(
            scalar("list(card, ns.mycard, 3, list(x))"),
            Value::Seq(vec![
                text("card"),
                text("ns.mycard"),
                Value::Int(3),
                Value::Seq(vec![text("x")]),
            ])
        );
    }

    #[test]
    fn string_takes_a_bare_argument_as_written() {
        for (src, want) in [
            ("string()", ""),
            ("string(a)", "a"),
            ("string(hello world)", "hello world"),
            ("string(1.50)", "1.50"),
            ("string(007)", "007"),
            ("string(true)", "true"),
            ("string(null)", "null"),
            ("string(inf)", "inf"),
            ("string(\"x y\")", "x y"),
            // out of range as a number, but it is only text here
            ("string(9223372036854775808)", "9223372036854775808"),
            ("string(int(5))", "5"),
            ("string(float(1))", "1.0"),
            ("string(float(nan))", "nan"),
            ("string(float(-inf))", "-inf"),
        ] {
            assert_eq!(scalar(src), text(want), "{src}");
        }
    }

    #[test]
    fn string_rejects_what_has_no_text() {
        for src in [
            "string(a, b)",
            "string(list(1))",
            "string(enum(a))",
            "string({a: 1})",
        ] {
            assert!(parse_value(src).is_err(), "{src}");
        }
    }

    #[test]
    fn int_converts_the_way_python_does() {
        for (src, want) in [
            ("int()", 0),
            ("int(5)", 5),
            ("int(-5)", -5),
            ("int(\"7\")", 7),
            ("int(007)", 7),
            ("int(+5)", 5),
            ("int(1.9)", 1),
            ("int(-1.9)", -1),
            ("int(1e3)", 1000),
            ("int(float(2))", 2),
        ] {
            assert_eq!(scalar(src), Value::Int(want), "{src}");
        }
    }

    #[test]
    fn int_rejects_what_does_not_convert() {
        for src in [
            "int(1, 2)",
            "int(\"1.5\")",
            "int(0x10)",
            "int(abc)",
            "int(true)",
            "int(null)",
            "int(list(1))",
            "int(1e30)",
            "int(float(inf))",
        ] {
            assert!(parse_value(src).is_err(), "{src}");
        }
    }

    #[test]
    fn a_constructor_error_points_at_the_call() {
        let err = parse_value("k: int(abc)").unwrap_err();
        assert!(err.message.contains("cannot convert `abc`"), "{err}");
        assert_eq!((err.line, err.column), (1, 4), "{err}");
    }

    #[test]
    fn a_callee_that_is_not_a_constructor_stays_a_call() {
        assert_eq!(
            scalar("enum(a, b)"),
            Value::Call("enum".into(), vec![text("a"), text("b")])
        );
        assert_eq!(
            scalar("other(1)"),
            Value::Call("other".into(), vec![Value::Int(1)])
        );
        // every table entry resolves, so none of them is ever a `Call`
        for name in LITERAL_CONSTRUCTORS {
            assert!(
                !matches!(scalar(&format!("{name}()")), Value::Call(..)),
                "{name}"
            );
        }
    }

    #[test]
    fn an_empty_document_is_an_empty_map() {
        for src in ["", "  \n\t\n", "// nothing here\n", "// a\n// b"] {
            assert_eq!(parse_value(src).unwrap(), Value::Map(vec![]), "{src:?}");
        }
        assert_eq!(print_value(&Value::Map(vec![])), "");
    }

    #[test]
    fn a_value_slot_still_needs_a_value() {
        // only the document is allowed to be empty; `k:` and `f()` arguments are not
        for src in ["k:", "k: // nothing\n", "k: list(,)"] {
            assert!(parse_value(src).is_err(), "{src:?}");
        }
    }
}
