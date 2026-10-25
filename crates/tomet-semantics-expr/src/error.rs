//! What can go wrong while evaluating an expression.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum EvalError {
    /// A name that no table has. Names are looked up by position and never
    /// fall through to another table.
    UnknownName(String),
    UnknownFunction(String),
    /// `value.member` where the value's type has no such member.
    UnknownMember {
        on: &'static str,
        member: String,
    },
    WrongArgCount {
        function: String,
        expected: String,
        got: usize,
    },
    /// A value of the wrong type for what was asked of it.
    TypeMismatch {
        what: String,
        expected: String,
        got: &'static str,
    },
    /// An ordering between two values that have none.
    NotComparable {
        left: &'static str,
        right: &'static str,
    },
    DivisionByZero,
    /// A name that exists only while a document is being created, used
    /// while rendering one.
    PhaseOnly(String),
    /// A variable that is declared, has no `default`, and was given no value.
    NoValue(String),
    /// A document, a note, a directory or an id that could not be found.
    Unresolved(String),
    /// Text that is not an address.
    BadAddress(String),
    /// A value that has no data representation where data was required.
    /// `type_name` is the value's own type; `is_vault_ref` says whether it is, or
    /// holds, a document, a directory or an element found through a vault.
    NotData {
        type_name: &'static str,
        is_vault_ref: bool,
    },
    /// Anything that needs its own sentence.
    Message(String),
    /// Syntax the parser reads and the evaluator does not run yet.
    NotYetSupported(&'static str),
}

impl EvalError {
    pub fn message(text: impl Into<String>) -> Self {
        EvalError::Message(text.into())
    }

    pub fn mismatch(
        what: impl Into<String>,
        expected: impl Into<String>,
        got: &'static str,
    ) -> Self {
        EvalError::TypeMismatch {
            what: what.into(),
            expected: expected.into(),
            got,
        }
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UnknownName(name) => write!(f, "unknown name `{name}`"),
            EvalError::UnknownFunction(name) => {
                if let Some(macro_name) = name.strip_prefix("macro.") {
                    write!(f, "unknown macro `macro.{macro_name}`")
                } else {
                    write!(f, "unknown function `{name}`")
                }
            }
            EvalError::UnknownMember { on, member } => {
                write!(f, "a {on} has no member `{member}`")
            }
            EvalError::WrongArgCount {
                function,
                expected,
                got,
            } => write!(f, "`{function}` expects {expected} argument(s), got {got}"),
            EvalError::TypeMismatch {
                what,
                expected,
                got,
            } => write!(f, "{what} expects {expected}, got a {got}"),
            EvalError::NotComparable { left, right } => {
                write!(f, "cannot order a {left} against a {right}")
            }
            EvalError::DivisionByZero => f.write_str("division by zero"),
            EvalError::PhaseOnly(name) => write!(
                f,
                "`{name}` can only be used when a document is being created, not when it is rendered"
            ),
            EvalError::NoValue(name) => write!(
                f,
                "`var.{name}` is declared but has no value: give it a `default` in `::vars`, \
                 or pass `--var {name}=...` when creating the document"
            ),
            EvalError::Unresolved(what) => write!(f, "{what}"),
            EvalError::BadAddress(text) => write!(f, "`{text}` is not an address"),
            EvalError::NotData { type_name, .. } => {
                write!(f, "a {type_name} cannot be stored as data")
            }
            EvalError::Message(text) => f.write_str(text),
            EvalError::NotYetSupported(what) => {
                write!(f, "{what} is not supported by the evaluator yet")
            }
        }
    }
}

impl std::error::Error for EvalError {}
