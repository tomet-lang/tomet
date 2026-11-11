//! What a type is built from: the arguments written for it, and the plain
//! data a `default` can be.
//!
//! These are plain values of this crate, not a tree of any language: whoever
//! reads a type out of a document, a setting or an expression turns what it
//! read into these and hands them to [`Type::build`](crate::Type::build).

use std::fmt;

use crate::Type;
use crate::text::write_quoted;

/// A plain value: what a `default` holds when it is written out, and what
/// `length` or `prefix` is when they are written.
#[derive(Debug, Clone, PartialEq)]
pub enum Datum {
    Null,
    Bool(bool),
    Int(i64),
    /// A finite number; [`Type::build`] refuses any other.
    Float(f64),
    Text(String),
    /// `(a, b, c)`.
    Seq(Vec<Datum>),
}

impl Datum {
    /// What it is, for a sentence: `` the integer `6` `` or `` the text "doc-" ``.
    pub fn describe(&self) -> String {
        match self {
            Datum::Null => "`null`".to_string(),
            Datum::Bool(b) => format!("`{b}`"),
            Datum::Int(n) => format!("the integer `{n}`"),
            Datum::Float(n) => format!("the number `{n:?}`"),
            Datum::Text(text) => format!("the text {}", quoted(text)),
            Datum::Seq(_) => "a list".to_string(),
        }
    }

    /// Whether every number in it is finite: a type's text form has no
    /// spelling for the others.
    pub(crate) fn is_finite(&self) -> bool {
        match self {
            Datum::Float(n) => n.is_finite(),
            Datum::Seq(items) => items.iter().all(Datum::is_finite),
            _ => true,
        }
    }
}

/// The text form: `null`, `true`, `6`, `1.5`, `"doc-"`, `("a", "b")`.
impl fmt::Display for Datum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Datum::Null => f.write_str("null"),
            Datum::Bool(b) => write!(f, "{b}"),
            Datum::Int(n) => write!(f, "{n}"),
            // The shortest text that reads back as the same number, with a `.`
            // or an exponent so that it does not read back as an integer.
            Datum::Float(n) => write!(f, "{n:?}"),
            Datum::Text(text) => f.write_str(&quoted(text)),
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

/// One argument of a type, as it was written: a type (`list(string)`'s item), `auto`, or a
/// plain value.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// A type, built already.
    Type(Type),
    /// `auto`. It is the value of `default` and of nothing else: a type that
    /// is given it anywhere else, or that has nothing to make up, is an error
    /// ([`TypeError`](crate::TypeError)).
    Auto,
    /// A plain value.
    Datum(Datum),
}

impl Arg {
    /// What it is, for a sentence.
    pub fn describe(&self) -> String {
        match self {
            Arg::Type(ty) => format!("the type `{ty}`"),
            Arg::Auto => "`auto`".to_string(),
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

/// What a value of the type is when none is written.
#[derive(Debug, Clone, PartialEq)]
pub enum DefaultValue {
    /// `default: auto`: the value is made up when it is needed. Which sort of
    /// value is for the type to say ([`Kind::auto`](crate::Kind::auto)); only a
    /// kind that has one is given this.
    Auto,
    /// A value written out. It is not checked against the type here: what a
    /// value of a type is depends on how it is held (as a datum, as text, as
    /// an evaluated value), which is for the holder to check.
    Given(Datum),
}

/// The text form: `auto` or the value.
impl fmt::Display for DefaultValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefaultValue::Auto => f.write_str("auto"),
            DefaultValue::Given(datum) => datum.fmt(f),
        }
    }
}
