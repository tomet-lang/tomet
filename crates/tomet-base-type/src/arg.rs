//! What a type is built from: the arguments written for it, and the plain
//! data a `default` can be.
//!
//! These are plain values of this crate, not a tree of any language: whoever
//! reads a type out of a document, a setting or an expression turns what it
//! read into these and hands them to [`Type::build`](crate::Type::build).

use std::fmt;

use tomet_scalar::Scalar;

use crate::Type;
use crate::text::quoted;

/// A plain value: what a `default` holds when it is written out, and what
/// `length` or `prefix` is when they are written.
///
/// A [`Scalar`] or a list of them, a [`Scalar::Number`] included: a number kept as written, so the
/// type's text form round-trips its own spelling and `length: 1.10` is refused as written, not
/// read as `1.1`.
#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Scalar(Scalar),
    /// `(a, b, c)`.
    Seq(Vec<Datum>),
}

impl From<Scalar> for Datum {
    fn from(s: Scalar) -> Self {
        Datum::Scalar(s)
    }
}

impl Datum {
    /// What it is, for a sentence: `` the integer `6` `` or `` the text "doc-" ``.
    pub fn describe(&self) -> String {
        match self {
            Datum::Scalar(s) => s.describe(),
            Datum::Seq(_) => "a list".to_string(),
        }
    }

    /// Whether every number in it is finite: a type's text form has no
    /// spelling for the others.
    pub(crate) fn is_finite(&self) -> bool {
        match self {
            Datum::Scalar(Scalar::Float(n)) => n.is_finite(),
            Datum::Scalar(Scalar::Number(text)) => text.parse::<f64>().is_ok_and(|f| f.is_finite()),
            Datum::Seq(items) => items.iter().all(Datum::is_finite),
            _ => true,
        }
    }
}

/// The text form: `none`, `auto`, `true`, `6`, `1.5`, `"doc-"`, `("a", "b")`.
impl fmt::Display for Datum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // A text is always quoted here, so that it never reads as a number or a keyword. A float
            // with no finite value has no bare word, and this form has no other spelling for it
            // (`Type::build` refuses one), so it falls back to its text.
            Datum::Scalar(Scalar::Text(text)) => f.write_str(&quoted(text)),
            Datum::Scalar(s) => f.write_str(&s.bare().or_else(|| s.text()).unwrap_or_default()),
            Datum::Seq(items) => match items.as_slice() {
                [] => f.write_str("()"),
                [only] => write!(f, "({only},)"),
                items => {
                    f.write_str("(")?;
                    for (i, item) in items.iter().enumerate() {
                        if i > 0 {
                            f.write_str(", ")?;
                        }
                        write!(f, "{item}")?;
                    }
                    f.write_str(")")
                }
            },
        }
    }
}

/// One argument of a type, as it was written: a type (`list(string)`'s item) or a
/// plain value.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// A type, built already.
    Type(Type),
    /// A plain value.
    Datum(Datum),
}

impl Arg {
    /// What it is, for a sentence.
    pub fn describe(&self) -> String {
        match self {
            Arg::Type(ty) => format!("the type `{ty}`"),
            Arg::Datum(datum) => datum.describe(),
        }
    }
}

impl From<Type> for Arg {
    fn from(ty: Type) -> Arg {
        Arg::Type(ty)
    }
}

impl From<Datum> for Arg {
    fn from(datum: Datum) -> Arg {
        Arg::Datum(datum)
    }
}
