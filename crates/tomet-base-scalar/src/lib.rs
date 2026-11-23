//! The scalar a document can hold -- `none`, `auto`, a boolean, an integer, a float, a number kept as written, a text -- and the one grammar that reads a bare word as one and writes one back.
//!
//! A scalar is typeless: it is readable with no schema (the tree is readable without evaluating anything).
//! A type is a set of values, so values sit below types; the type crate depends on this one, never the reverse.
//!
//! A scalar is written three ways, for three purposes, and each has one place:
//!
//! - its content text, [`Scalar::text`]: what a scalar says in output. `none` says nothing, so a caller that wants an
//!   empty text for it maps that itself;
//! - its source spelling: [`Scalar::bare`] is the one bare word that [`Scalar::from_bare`] reads back as the same
//!   scalar, so the writer cannot drift from the reader. The part that needs a grammar (the quotes around a text that is
//!   not bare, a float with no finite value) is `tomet-spelling`'s;
//! - its description, [`Scalar::describe`]: what a message calls a value, as in `` the integer `6` ``.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// The scalar values a document can hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Scalar {
    /// `none`: the value is nothing, and says so. It is not an absent key, which says nothing, and not
    /// `auto`, which leaves the value to the system. Plain JSON data writes it as `null`; serialized as a scalar it is `"None"`.
    None,
    /// `auto`: the system supplies the value. A field takes it only where its schema says so, and a later layer
    /// can write it to put a field back in the system's hands. Serialized it is its own value (`"Auto"`), neither
    /// `null` (that is `none`) nor the text `auto`: JSON data has no value for it.
    Auto,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// A number kept as written, because reading it as an `i64` or `f64` would change it:
    /// `1.10` (an `f64` prints `1.1`), `1e3`, `-0`, or an integer too large for an `i64`.
    Number(String),
    Text(String),
}

/// What [`Scalar::from_bare`] refuses: a bare `null`. A document says nothing with `none`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BareNull;

impl std::fmt::Display for BareNull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a bare `null` is an error: write `none`")
    }
}

impl std::error::Error for BareNull {}

/// What a bare scalar's text is, by the JSON number grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumberShape {
    Int,
    Float,
}

/// Classifies a string according to the JSON number grammar.
fn classify_number(s: &str) -> Option<NumberShape> {
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        (i > start).then_some(i)
    };
    let mut i = usize::from(b.first() == Some(&b'-'));
    match b.get(i)? {
        b'0' => i += 1,
        b'1'..=b'9' => i = digits(i)?,
        _ => return None,
    }
    let mut float = false;
    if b.get(i) == Some(&b'.') {
        i = digits(i + 1)?;
        float = true;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        i = digits(i)?;
        float = true;
    }
    (i == b.len()).then_some(if float {
        NumberShape::Float
    } else {
        NumberShape::Int
    })
}

/// How an `f64` is written: the shortest text that reads back as the same number,
/// always with a `.` or an exponent.
pub fn float_text(f: f64) -> String {
    format!("{f:?}")
}

impl Scalar {
    /// Whether `s` is a keyword that stands for a value (`none`, `auto`, `true`, `false`):
    /// a word [`from_bare`](Scalar::from_bare) reads as one of those and not as a number or text.
    /// The keywords are written once, in `from_bare`.
    pub fn is_value_keyword(s: &str) -> bool {
        matches!(
            Scalar::from_bare(s),
            Ok(Scalar::None | Scalar::Auto | Scalar::Bool(_))
        )
    }

    /// Reads a bare word as a scalar value.
    ///
    /// - `null` is [`BareNull`].
    /// - `none`, `auto`, `true`, `false` produce the corresponding variant.
    /// - Numbers following the JSON number grammar produce [`Scalar::Int`], [`Scalar::Float`],
    ///   or [`Scalar::Number`] if reading would alter the text (such as `-0`, `1.10`, `1e3`,
    ///   or values exceeding `i64`).
    /// - Any other word produces [`Scalar::Text`].
    pub fn from_bare(s: &str) -> Result<Self, BareNull> {
        match s {
            "null" => return Err(BareNull),
            "none" => return Ok(Scalar::None),
            "auto" => return Ok(Scalar::Auto),
            "true" => return Ok(Scalar::Bool(true)),
            "false" => return Ok(Scalar::Bool(false)),
            _ => {}
        }
        match classify_number(s) {
            Some(NumberShape::Int) => Ok(match s.parse::<i64>() {
                Ok(i) if i.to_string() == s => Scalar::Int(i),
                _ => Scalar::Number(s.to_string()),
            }),
            Some(NumberShape::Float) => Ok(match s.parse::<f64>() {
                Ok(f) if f.is_finite() && float_text(f) == s => Scalar::Float(f),
                _ => Scalar::Number(s.to_string()),
            }),
            None => Ok(Scalar::Text(s.to_string())),
        }
    }

    /// The text a scalar was written as.
    ///
    /// Returns `None` for [`Scalar::None`], as `none` is absence and has no string text.
    pub fn text(&self) -> Option<Cow<'_, str>> {
        Some(match self {
            Scalar::Auto => Cow::Borrowed("auto"),
            Scalar::Text(s) | Scalar::Number(s) => Cow::Borrowed(s.as_str()),
            Scalar::Int(i) => Cow::Owned(i.to_string()),
            Scalar::Float(f) => Cow::Owned(float_text(*f)),
            Scalar::Bool(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
            Scalar::None => return None,
        })
    }

    /// The one bare word that [`from_bare`](Scalar::from_bare) reads back as this scalar, or `None` when no bare word
    /// does: a text that would read as something else (`true`, `1.5`, `none`) or as nothing (empty), and a float with
    /// no finite value.
    ///
    /// `from_bare(bare(s)) == s` for every `s` that `from_bare` produces. (A [`Scalar::Number`] built by hand from a
    /// canonical text, `Number("8")`, reads back as the integer it is.)
    pub fn bare(&self) -> Option<Cow<'_, str>> {
        match self {
            Scalar::None => Some(Cow::Borrowed("none")),
            Scalar::Float(f) if !f.is_finite() => None,
            Scalar::Text(s)
                if s.is_empty() || !matches!(Scalar::from_bare(s), Ok(Scalar::Text(_))) =>
            {
                None
            }
            _ => self.text(),
        }
    }

    /// What a message calls this value: `` `none` ``, `` the integer `6` ``, `` the number `1.5` ``,
    /// `` the text "doc-" ``.
    pub fn describe(&self) -> String {
        match self {
            Scalar::None => "`none`".to_string(),
            Scalar::Auto => "`auto`".to_string(),
            Scalar::Bool(b) => format!("`{b}`"),
            Scalar::Int(n) => format!("the integer `{n}`"),
            Scalar::Float(f) => format!("the number `{}`", float_text(*f)),
            Scalar::Number(text) => format!("the number `{text}`"),
            Scalar::Text(text) => format!("the text {text:?}"),
        }
    }

    /// The number a [`Scalar::Float`] or [`Scalar::Number`] is, as an `f64`.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Scalar::Float(f) => Some(*f),
            Scalar::Number(text) => text.parse::<f64>().ok().filter(|f| f.is_finite()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_word_scalar_table() {
        let cases: &[(&str, Result<Scalar, &str>)] = &[
            ("null", Err("a bare `null` is an error: write `none`")),
            ("none", Ok(Scalar::None)),
            ("auto", Ok(Scalar::Auto)),
            ("true", Ok(Scalar::Bool(true))),
            ("false", Ok(Scalar::Bool(false))),
            ("42", Ok(Scalar::Int(42))),
            ("-42", Ok(Scalar::Int(-42))),
            ("0", Ok(Scalar::Int(0))),
            ("-0", Ok(Scalar::Number("-0".to_string()))),
            ("1.5", Ok(Scalar::Float(1.5))),
            ("-1.5", Ok(Scalar::Float(-1.5))),
            ("1.10", Ok(Scalar::Number("1.10".to_string()))),
            ("1e3", Ok(Scalar::Number("1e3".to_string()))),
            (
                "999999999999999999999999999999",
                Ok(Scalar::Number("999999999999999999999999999999".to_string())),
            ),
            ("1.10.0", Ok(Scalar::Text("1.10.0".to_string()))),
            ("007", Ok(Scalar::Text("007".to_string()))),
            ("2026-10-25", Ok(Scalar::Text("2026-10-25".to_string()))),
            ("hello", Ok(Scalar::Text("hello".to_string()))),
        ];

        for (src, want) in cases {
            let got = Scalar::from_bare(src);
            match want {
                Ok(expected) => assert_eq!(&got.unwrap(), expected, "case {src}"),
                Err(err_msg) => assert_eq!(got.unwrap_err().to_string(), *err_msg, "case {src}"),
            }
        }
    }

    /// The writer is held to the reader: every word `from_bare` reads is written back by `bare` as a word that reads
    /// as the same scalar.
    #[test]
    fn bare_is_the_inverse_of_from_bare() {
        for word in [
            "none",
            "auto",
            "true",
            "false",
            "0",
            "42",
            "-42",
            "-0",
            "1.5",
            "-1.5",
            "1.10",
            "1e3",
            "999999999999999999999999999999",
            "1.10.0",
            "007",
            "2026-10-25",
            "hello",
        ] {
            let scalar = Scalar::from_bare(word).unwrap();
            let written = scalar
                .bare()
                .unwrap_or_else(|| panic!("`{word}` has a bare word"));
            assert_eq!(Scalar::from_bare(&written).unwrap(), scalar, "`{word}`");
        }
    }

    #[test]
    fn bare_is_none_where_no_bare_word_reads_back() {
        for scalar in [
            Scalar::Text("true".into()),
            Scalar::Text("none".into()),
            Scalar::Text("1.5".into()),
            Scalar::Text(String::new()),
            Scalar::Float(f64::NAN),
            Scalar::Float(f64::INFINITY),
            Scalar::Float(f64::NEG_INFINITY),
        ] {
            assert_eq!(scalar.bare(), None, "{scalar:?}");
        }
        assert_eq!(Scalar::None.bare().as_deref(), Some("none"));
        assert_eq!(Scalar::Float(1.0).bare().as_deref(), Some("1.0"));
    }

    #[test]
    fn describe_says_what_a_value_is() {
        assert_eq!(Scalar::None.describe(), "`none`");
        assert_eq!(Scalar::Auto.describe(), "`auto`");
        assert_eq!(Scalar::Bool(true).describe(), "`true`");
        assert_eq!(Scalar::Int(6).describe(), "the integer `6`");
        assert_eq!(Scalar::Float(1.0).describe(), "the number `1.0`");
        assert_eq!(
            Scalar::Number("1.10".into()).describe(),
            "the number `1.10`"
        );
        assert_eq!(Scalar::Text("doc-".into()).describe(), "the text \"doc-\"");
    }

    #[test]
    fn scalar_text_representations() {
        assert_eq!(Scalar::None.text(), None);
        assert_eq!(Scalar::Auto.text().as_deref(), Some("auto"));
        assert_eq!(Scalar::Bool(true).text().as_deref(), Some("true"));
        assert_eq!(Scalar::Bool(false).text().as_deref(), Some("false"));
        assert_eq!(Scalar::Int(42).text().as_deref(), Some("42"));
        assert_eq!(Scalar::Float(1.5).text().as_deref(), Some("1.5"));
        assert_eq!(
            Scalar::Number("1.10".into()).text().as_deref(),
            Some("1.10")
        );
        assert_eq!(
            Scalar::Text("hello".into()).text().as_deref(),
            Some("hello")
        );
    }
}
