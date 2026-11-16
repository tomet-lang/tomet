//! What a type is built from: the arguments written for it, and the plain
//! data a `default` can be.
//!
//! These are plain values of this crate, not a tree of any language: whoever
//! reads a type out of a document, a setting or an expression turns what it
//! read into these and hands them to [`Type::build`](crate::Type::build).

use std::fmt;

use tomet_scalar::Scalar;

use crate::Type;
use crate::text::write_quoted;

/// A plain value: what a `default` holds when it is written out, and what
/// `length` or `prefix` is when they are written.
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
            Datum::Scalar(s) => match s {
                Scalar::None => "`none`".to_string(),
                Scalar::Auto => "`auto`".to_string(),
                Scalar::Bool(b) => format!("`{b}`"),
                Scalar::Int(n) => format!("the integer `{n}`"),
                Scalar::Float(n) => format!("the number `{n:?}`"),
                Scalar::Number(n) => format!("the number `{n}`"),
                Scalar::Text(text) => format!("the text {}", quoted(text)),
            },
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
            Datum::Scalar(s) => match s {
                Scalar::None => f.write_str("none"),
                Scalar::Auto => f.write_str("auto"),
                Scalar::Bool(b) => write!(f, "{b}"),
                Scalar::Int(n) => write!(f, "{n}"),
                Scalar::Float(n) => write!(f, "{n:?}"),
                Scalar::Number(n) => f.write_str(n),
                Scalar::Text(text) => f.write_str(&quoted(text)),
            },
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

fn quoted(text: &str) -> String {
    let mut out = String::new();
    write_quoted(text, &mut out);
    out
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
