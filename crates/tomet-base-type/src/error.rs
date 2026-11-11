//! Why a type could not be built or read.

use crate::Kind;

/// What was written for a type is not a type.
///
/// Every message names the type and the argument it is about, because it is
/// shown to whoever wrote the declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    /// The value was not a type at all: not a name and not a call, but a
    /// number, a list, a map or another structure.
    NotAType,
    /// The name is not a type.
    Unknown(String),
    /// The text form is not well formed: `expected` is what should have been
    /// at byte `at` of `text`.
    Malformed {
        text: String,
        at: usize,
        expected: &'static str,
    },
    /// The type takes no argument of this name.
    UnknownArgument { ty: Kind, argument: String },
    /// The same named argument was written twice.
    DuplicateArgument { ty: Kind, argument: String },
    /// More arguments were written by position than the type takes.
    TooManyArguments {
        ty: Kind,
        takes: usize,
        given: usize,
    },
    /// An argument is not what the parameter takes.
    WrongArgument {
        ty: Kind,
        /// `item`, `length`, `prefix` or `default`.
        argument: &'static str,
        /// What the argument takes, for a sentence.
        expected: String,
        /// What was given, for a sentence.
        found: String,
    },
    /// `default: auto` for a type that has no value to make up.
    AutoNotAllowed { ty: Kind },
    /// `auto` as the value of an argument that is not `default`.
    AutoElsewhere { ty: Kind, argument: String },
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeError::Unknown(name) => {
                write!(f, "`{name}` is not a type; the types are {}", types())
            }
            TypeError::NotAType => write!(
                f,
                "a type is written as a name, such as `string`, or as a name with arguments, \
                 such as `list(string)`; the types are {}",
                types()
            ),
            TypeError::Malformed { text, at, expected } => {
                write!(
                    f,
                    "`{text}` is not a type: expected {expected} at byte {at}"
                )
            }
            TypeError::UnknownArgument { ty, argument } => write!(
                f,
                "`{ty}` has no argument `{argument}`; it takes {}",
                arguments(*ty)
            ),
            TypeError::DuplicateArgument { ty, argument } => {
                write!(f, "`{ty}`: the argument `{argument}` is written twice")
            }
            TypeError::TooManyArguments { ty, takes: 0, .. } => write!(
                f,
                "`{ty}` takes no argument written by position, only named ones: {}",
                arguments(*ty)
            ),
            TypeError::TooManyArguments { ty, takes, given } => write!(
                f,
                "`{ty}` takes {takes} argument written by position (its item type), \
                 and {given} are written"
            ),
            TypeError::WrongArgument {
                ty,
                argument,
                expected,
                found,
            } => write!(f, "`{ty}`: `{argument}` takes {expected}, found {found}"),
            TypeError::AutoNotAllowed { ty } => {
                let makers: Vec<String> = Kind::ALL
                    .into_iter()
                    .filter(|kind| kind.auto().is_some())
                    .map(|kind| format!("`{kind}`"))
                    .collect();
                write!(
                    f,
                    "`{ty}` has no `default: auto`; only {} make a value up",
                    list(&makers)
                )
            }
            TypeError::AutoElsewhere { ty, argument } => write!(
                f,
                "`{ty}`: `auto` is the value of `default` and of nothing else, not of `{argument}`"
            ),
        }
    }
}

impl std::error::Error for TypeError {}

fn types() -> String {
    let names: Vec<String> = Kind::ALL
        .into_iter()
        .map(|kind| format!("`{kind}`"))
        .collect();
    names.join(", ")
}

/// The arguments a kind takes, for a sentence: `` an item type, `length` and `default` ``.
fn arguments(kind: Kind) -> String {
    let mut parts = Vec::new();
    if kind.positional() > 0 {
        parts.push("an item type".to_string());
    }
    parts.extend(kind.named().iter().map(|name| format!("`{name}`")));
    list(&parts)
}

/// `a`, `a and b`, `a, b and c`.
fn list(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}
