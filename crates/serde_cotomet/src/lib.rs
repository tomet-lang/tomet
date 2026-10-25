//! =[ serde_cotomet ]
//!
//! Serde support for cotomet: read a cotomet document into a Rust type and write a
//! Rust type out as one. The language itself is `cotomet-syntax`; depend on
//! `cotomet` instead unless you are a crate below it.
//!
//! =[ Key Functions ]
//!
//! - (from_str) Parse a cotomet data document into a Rust type via Serde.
//! - (to_string) Render a Rust type into cotomet data text via Serde.
//! - (from_value) Convert a [`Value`] into a Rust type via Serde deserializer.
//! - (to_value) Convert a Rust type into a [`Value`] via Serde serializer.
//!
//! =[ Errors ]
//!
//! [`Error::Syntax`] is text that is not cotomet, with a line and column.
//! [`Error::Data`] is a value that does not fit the Rust type, with the path
//! from the document root to that value, written the way the document spells
//! it: `bookmarks[1].path: invalid type: integer `1`, expected a string`. A
//! key that is not a bare identifier is quoted (`"a/b"[1]`), and so is one that holds a
//! `.`, which would read as a step (`"a.tmt".source`). An enum
//! variant is a key (`effect.Blur`). A `missing field` error is at the map
//! that lacks the field. There is no line or column on a data error, because a
//! [`Value`] carries no positions.

pub mod de;
pub mod error;
pub mod ser;

pub use cotomet_syntax::{Value, print_value};
pub use de::from_value;
pub use error::{Error, Result, Segment};
pub use ser::to_value;

/// Parse a cotomet data document into `T` via Serde.
pub fn from_str<T: for<'de> serde::Deserialize<'de>>(src: &str) -> Result<T> {
    de::from_value(cotomet_syntax::parse_value(src)?)
}

/// Render `T` into cotomet data text via Serde.
pub fn to_string<T: serde::Serialize + ?Sized>(value: &T) -> Result<String> {
    let v = ser::to_value(value)?;
    Ok(print_value(&v))
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
            "enum".into(),
            vec![Value::String("card".into())],
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("call syntax"), "unexpected message: {msg}");
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
    fn an_unsigned_integer_beyond_i64_is_kept_as_its_digits_not_wrapped() {
        assert_eq!(to_string(&(i64::MAX as u64)).unwrap(), i64::MAX.to_string());
        assert_eq!(to_string(&u64::MAX).unwrap(), u64::MAX.to_string());
        assert_eq!(from_str::<u64>(&u64::MAX.to_string()).unwrap(), u64::MAX);
        assert_eq!(to_string(&u128::MAX).unwrap(), u128::MAX.to_string());
        assert_eq!(from_str::<u128>(&u128::MAX.to_string()).unwrap(), u128::MAX);
    }

    #[test]
    fn a_string_target_takes_a_bare_scalar_as_the_text_it_was_written_as() {
        // the revision that used to need quotes: `rev: 1.10` was read as 1.1
        #[derive(Debug, PartialEq, serde::Deserialize)]
        struct Rev {
            rev: String,
        }
        for (src, want) in [
            ("rev: 1.10", "1.10"),
            ("rev: 1.0", "1.0"),
            ("rev: 1e3", "1e3"),
            ("rev: 2024", "2024"),
            ("rev: true", "true"),
            ("rev: 1.10.0", "1.10.0"),
            ("rev: \"1.10\"", "1.10"),
        ] {
            assert_eq!(from_str::<Rev>(src).unwrap().rev, want, "{src}");
        }
        // a float target still reads the number
        assert_eq!(from_str::<f64>("1.10").unwrap(), 1.1);
        assert_eq!(from_str::<i64>("-0").unwrap(), 0);
    }

    #[test]
    fn a_list_reads_into_a_vec() {
        assert_eq!(
            from_str::<Vec<i32>>("list(1, 2, 3)").unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn an_empty_document_fills_every_optional_field() {
        let all_optional: std::collections::BTreeMap<String, i32> = from_str("").unwrap();
        assert!(all_optional.is_empty());
        #[derive(Debug, PartialEq, Deserialize)]
        struct Opt {
            a: Option<i32>,
        }
        assert_eq!(from_str::<Opt>("// nothing\n").unwrap(), Opt { a: None });
    }

    fn failure<T: for<'de> Deserialize<'de> + std::fmt::Debug>(src: &str) -> String {
        from_str::<T>(src).unwrap_err().to_string()
    }

    #[test]
    fn a_data_error_names_the_key_path_to_the_value() {
        let err = failure::<Settings>("bookmarks: list({label: a, path: list(1)})");
        assert!(err.starts_with("bookmarks[0].path: "), "{err}");
        assert!(err.contains("invalid type"), "{err}");

        let err = failure::<Settings>("refresh_ms: x\nbookmarks: list()");
        assert!(err.starts_with("refresh_ms: "), "{err}");
    }

    #[test]
    fn a_missing_field_is_reported_at_the_map_that_lacks_it() {
        let err = failure::<Settings>("bookmarks: list({label: a, path: p}, {label: b})");
        assert!(err.starts_with("bookmarks[1]: "), "{err}");
        assert!(err.contains("missing field `path`"), "{err}");

        // at the top level there is no path to name
        let err = failure::<Settings>("style: x");
        assert!(err.starts_with("missing field"), "{err}");
    }

    #[test]
    fn a_key_that_needs_quotes_is_quoted_in_the_path() {
        use std::collections::BTreeMap;
        let err = failure::<BTreeMap<String, Vec<i32>>>(r#""a/b": list(1, x)"#);
        assert!(err.starts_with("\"a/b\"[1]: "), "{err}");
    }

    #[test]
    fn a_key_holding_a_dot_is_quoted_so_it_is_not_taken_for_two_steps() {
        use std::collections::BTreeMap;
        // `a.tmt` is a bare key, but in a path the dot separates steps
        let err = failure::<BTreeMap<String, Bookmark>>("a.tmt: { label: x }");
        assert!(err.starts_with("\"a.tmt\": "), "{err}");
        assert!(err.contains("missing field `path`"), "{err}");

        let err = failure::<BTreeMap<String, Bookmark>>("a.tmt: { label: x, path: list(1) }");
        assert!(err.starts_with("\"a.tmt\".path: "), "{err}");

        // a key without a dot stays bare
        let err = failure::<BTreeMap<String, Bookmark>>("a-b: { label: x, path: list(1) }");
        assert!(err.starts_with("a-b.path: "), "{err}");
    }

    #[test]
    fn an_enum_variant_is_part_of_the_path() {
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        enum Effect {
            Blur(u32),
        }
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Wrapper {
            effect: Effect,
        }
        let err = failure::<Wrapper>("effect: { Blur: big }");
        assert!(err.starts_with("effect.Blur: "), "{err}");
    }

    #[test]
    fn the_path_is_data_not_just_text() {
        let Error::Data { path, .. } =
            from_str::<Settings>("bookmarks: list({label: a, path: list(1)})").unwrap_err()
        else {
            panic!("expected a data error");
        };
        assert_eq!(
            path,
            [
                Segment::Key("bookmarks".into()),
                Segment::Index(0),
                Segment::Key("path".into())
            ]
        );
    }

    #[test]
    fn a_syntax_error_keeps_its_position_and_has_no_path() {
        let err = from_str::<Settings>("a: 1\nb: \"x").unwrap_err();
        assert!(matches!(err, Error::Syntax(ref e) if e.line == 2), "{err}");
    }
}
