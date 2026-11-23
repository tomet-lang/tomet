//! Parsing and formatting `.cache.ctmt` cache entry documents.

use std::fmt;

use tomet_ast::{Tuple, Value};
use tomet_declaration::document_kind;
use tomet_position::LineIndex;
use tomet_scalar::Scalar;
use tomet_span::Span;

/// A parsed `.cache.ctmt` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheEntry {
    /// The original cache key string stored in `key:`.
    pub key: String,
    /// The cached Cotomet data value stored in `value:`.
    pub value: Value,
    /// Optional producer metadata stored in `meta:`.
    pub meta: Option<Value>,
}

/// Why a `.cache.ctmt` document could not be read as a valid cache entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheParseError {
    /// Syntax error or non-data construct (such as `$expr` or `<$type>`) in the `.ctmt` file.
    Syntax(String),
    /// Missing or non-`cache` `::kind(...)` declaration.
    WrongKind { found: Option<String> },
    /// The top-level body is not a key-value map.
    NotAMap,
    /// The required `key:` field is missing.
    MissingKey,
    /// The `key:` field is not a string scalar.
    InvalidKeyType,
    /// The `key:` field does not match the requested key (cache corruption / hash collision).
    KeyMismatch { expected: String, found: String },
    /// The required `value:` field is missing.
    MissingValue,
}

impl fmt::Display for CacheParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CacheParseError::Syntax(msg) => write!(f, "invalid cache syntax: {msg}"),
            CacheParseError::WrongKind { found: Some(kind) } => {
                write!(f, "expected `::kind(cache)`, found `::kind({kind})`")
            }
            CacheParseError::WrongKind { found: None } => {
                write!(f, "expected `::kind(cache)`, found no `::kind` declaration")
            }
            CacheParseError::NotAMap => {
                write!(f, "cache entry body must be a key-value map")
            }
            CacheParseError::MissingKey => {
                write!(f, "cache entry is missing required `key` field")
            }
            CacheParseError::InvalidKeyType => {
                write!(f, "cache entry `key` field must be a string")
            }
            CacheParseError::KeyMismatch { expected, found } => {
                write!(
                    f,
                    "cache key mismatch: expected `{expected}`, found `{found}`"
                )
            }
            CacheParseError::MissingValue => {
                write!(f, "cache entry is missing required `value` field")
            }
        }
    }
}

impl std::error::Error for CacheParseError {}

/// Parses a `.cache.ctmt` document, verifying `::kind(cache)`, `key`, and `value`.
///
/// When `expected_key` is `Some(expected)`, verifies that the entry's `key` matches
/// `expected` exactly; mismatches are reported as [`CacheParseError::KeyMismatch`].
/// Additional producer fields such as `meta` or custom keys are allowed; `meta` is
/// returned in [`CacheEntry::meta`] and any other fields are ignored.
pub fn parse_cache_entry(
    src: &str,
    expected_key: Option<&str>,
) -> Result<CacheEntry, CacheParseError> {
    let file =
        tomet_ast_parser::parse_data_file(src).map_err(|e| CacheParseError::Syntax(e.to_string()))?;

    if let Some((span, message)) = find_not_data(&file.body) {
        let at = LineIndex::new(src).line_col(span.start);
        return Err(CacheParseError::Syntax(format!(
            "{}:{}: {message}",
            at.line, at.column
        )));
    }

    let kind = document_kind(&file);
    if kind.as_deref() != Some("cache") {
        return Err(CacheParseError::WrongKind { found: kind });
    }

    let Value::Tuple(tuple) = &file.body else {
        return Err(CacheParseError::NotAMap);
    };

    let key_val = tuple.get("key").ok_or(CacheParseError::MissingKey)?;
    let Value::Scalar(Scalar::Text(key)) = key_val else {
        return Err(CacheParseError::InvalidKeyType);
    };

    if let Some(expected) = expected_key
        && key != expected
    {
        return Err(CacheParseError::KeyMismatch {
            expected: expected.to_string(),
            found: key.clone(),
        });
    }

    let value = tuple
        .get("value")
        .cloned()
        .ok_or(CacheParseError::MissingValue)?;
    let meta = tuple.get("meta").cloned();

    Ok(CacheEntry {
        key: key.clone(),
        value,
        meta,
    })
}

/// Formats `entry` as a valid `.cache.ctmt` document (`::kind(cache)`, `key`, `value`,
/// and optional `meta`).
pub fn format_cache_entry(entry: &CacheEntry) -> String {
    let mut entries = vec![
        (
            "key".to_string(),
            Value::Scalar(Scalar::Text(entry.key.clone())),
        ),
        ("value".to_string(), entry.value.clone()),
    ];
    if let Some(meta) = &entry.meta {
        entries.push(("meta".to_string(), meta.clone()));
    }
    let body = serde_cotomet::print_value(&Value::Tuple(Tuple::from_named(entries)));
    format!("::kind(cache)\n\n{body}\n")
}

/// Rejects non-data constructs (`$expr`, `<$type>`) inside a `.cache.ctmt` body.
fn find_not_data(v: &Value) -> Option<(Span, &'static str)> {
    match v {
        Value::Interp(interp) => Some((
            interp.span,
            "an expression (`{$ ... }`, `$(...)`, `$name(...)`) is not valid in cache data",
        )),
        Value::TypeSeat(seat) => Some((
            seat.span,
            "a type seat (`<$type>`) is not valid in cache data",
        )),
        Value::Tuple(tuple) | Value::Call(_, tuple) => tuple.entries().find_map(|entry| {
            if let Some(seat) = entry.type_seat_key() {
                return Some((
                    seat.span,
                    "a type seat (`<$type>`) is not valid in cache data",
                ));
            }
            find_not_data(entry.value())
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::Tuple;

    #[test]
    fn parses_spec_example_with_meta() {
        let src = r#"::kind(cache)

key:   "wc -l < scripts/deploy.sh"
value: "42"
meta:  (
  inputs: (
    "scripts/deploy.sh": "e3b0c442"
  )
)
"#;
        let entry = parse_cache_entry(src, Some("wc -l < scripts/deploy.sh")).unwrap();
        assert_eq!(entry.key, "wc -l < scripts/deploy.sh");
        assert_eq!(entry.value, Value::Scalar(Scalar::Text("42".into())));
        assert!(entry.meta.is_some());
    }

    #[test]
    fn ignores_extra_producer_fields() {
        let src = r#"::kind(cache)

key: "whoami"
value: "alice"
producer: "tetch"
exit_code: 0
"#;
        let entry = parse_cache_entry(src, Some("whoami")).unwrap();
        assert_eq!(entry.key, "whoami");
        assert_eq!(entry.value, Value::Scalar(Scalar::Text("alice".into())));
        assert_eq!(entry.meta, None);
    }

    #[test]
    fn round_trips_multiline_string_maps_tuples_and_meta() {
        let cases = vec![
            CacheEntry {
                key: "tomet --help".into(),
                value: Value::Scalar(Scalar::Text(
                    "line 1\nline 2 with \"quotes\" and \\backslash\\\n\tindented".into(),
                )),
                meta: None,
            },
            CacheEntry {
                key: "git status".into(),
                value: Value::Tuple(Tuple::from_named([
                    ("stdout".into(), Value::Scalar(Scalar::Text("clean".into()))),
                    ("code".into(), Value::Scalar(Scalar::Int(0))),
                    ("ok".into(), Value::Scalar(Scalar::Bool(true))),
                    (
                        "lines".into(),
                        Value::Tuple(Tuple::from_unnamed([
                            Value::Scalar(Scalar::Text("a".into())),
                            Value::Scalar(Scalar::Text("b".into())),
                        ])),
                    ),
                ])),
                meta: Some(Value::Tuple(Tuple::from_named([(
                    "inputs".into(),
                    Value::Tuple(Tuple::from_named([(
                        "scripts/deploy.sh".into(),
                        Value::Scalar(Scalar::Text("e3b0c442".into())),
                    )])),
                )]))),
            },
            CacheEntry {
                key: "empty-tuple".into(),
                value: Value::Tuple(Tuple::new()),
                meta: None,
            },
        ];

        for entry in cases {
            let formatted = format_cache_entry(&entry);
            let parsed = parse_cache_entry(&formatted, Some(&entry.key))
                .unwrap_or_else(|e| panic!("failed to round-trip {:?}: {e}\n{formatted}", entry.key));
            assert_eq!(parsed, entry);
        }
    }

    #[test]
    fn rejects_wrong_kind_missing_fields_and_key_mismatch() {
        assert_eq!(
            parse_cache_entry("key: \"a\"\nvalue: \"b\"\n", None),
            Err(CacheParseError::WrongKind { found: None })
        );
        assert_eq!(
            parse_cache_entry("::kind(note)\nkey: \"a\"\nvalue: \"b\"\n", None),
            Err(CacheParseError::WrongKind {
                found: Some("note".into())
            })
        );
        assert_eq!(
            parse_cache_entry("::kind(cache)\nvalue: \"b\"\n", None),
            Err(CacheParseError::MissingKey)
        );
        assert_eq!(
            parse_cache_entry("::kind(cache)\nkey: 42\nvalue: \"b\"\n", None),
            Err(CacheParseError::InvalidKeyType)
        );
        assert_eq!(
            parse_cache_entry("::kind(cache)\nkey: \"a\"\n", None),
            Err(CacheParseError::MissingValue)
        );
        assert_eq!(
            parse_cache_entry("::kind(cache)\nkey: \"actual\"\nvalue: \"b\"\n", Some("expected")),
            Err(CacheParseError::KeyMismatch {
                expected: "expected".into(),
                found: "actual".into()
            })
        );
    }

    #[test]
    fn rejects_corrupt_syntax_and_unquoted_expressions() {
        assert!(matches!(
            parse_cache_entry("::kind(cache)\nkey: \"unterminated\n", None),
            Err(CacheParseError::Syntax(_))
        ));
        assert!(matches!(
            parse_cache_entry("::kind(cache)\nkey: \"a\"\nvalue: $macro.foo()\n", None),
            Err(CacheParseError::Syntax(_))
        ));
        assert!(matches!(
            parse_cache_entry("::kind(cache)\nkey: \"a\"\nvalue: <$string>\n", None),
            Err(CacheParseError::Syntax(_))
        ));
    }
}
