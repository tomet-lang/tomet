//! =[ tomet-type ]
//!
//! The types a value can be declared as, and how text is held to them.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (One vocabulary)
//!   | A `type:` is written in the settings' `meta` fields, in `::var`, and in
//!   | a vocabulary's arguments. They all name a [`Type`] here, so a name that
//!   | one place accepts is a name every place accepts.
//! - (A pure base-layer crate)
//!   | This crate is in the base layer (`crates/tomet-base-*`) and depends on
//!   | nothing in the workspace. A type does not know what an element is, what
//!   | an AST node is, or which vocabulary declared it.
//! - (What is held to a type)
//!   | [`Type::holds_text`] asks whether a string is in the form the type gives
//!   | text, for the types that have one.

use std::borrow::Cow;

pub mod datetime;
mod version;

pub use version::{Identifier, Version};

/// A type a value can be declared as.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// Any text.
    String,
    /// A whole number.
    Int,
    /// A whole number that is not negative.
    Uint,
    /// A number; a whole number is one.
    Float,
    /// `true` or `false`.
    Bool,
    /// A date, or a date and a time ([`datetime::parse_datetime_parts`]).
    Datetime,
    /// A version: dotted non-negative integers (`1`, `1.0`, `1.10.0`), with
    /// optional pre-release (`-beta`) and build metadata (`+5`) ([`Version`]).
    /// It is text. A bare `1.10` is read as the text it was written as, not as
    /// the number it would be.
    Version,
    /// An identifier in the 8-4-4-4-12 hexadecimal form.
    Uuid,
    /// A short generated identifier. Its form depends on the field's `prefix`
    /// and `length`, which are not the type's; this admits any string.
    Nanoid,
    /// A list of values, optionally specifying the element type: `list` or `list(string)`.
    List(Option<Box<Type>>),
}

impl Type {
    /// Every base type, in the order they are listed to people.
    pub const ALL: [Type; 10] = [
        Type::String,
        Type::Int,
        Type::Uint,
        Type::Float,
        Type::Bool,
        Type::Datetime,
        Type::Version,
        Type::Uuid,
        Type::Nanoid,
        Type::List(None),
    ];

    /// The base name it is written as in a `type:` (e.g. `"list"` for any list).
    pub fn base_name(&self) -> &'static str {
        match self {
            Type::String => "string",
            Type::Int => "int",
            Type::Uint => "uint",
            Type::Float => "float",
            Type::Bool => "bool",
            Type::Datetime => "datetime",
            Type::Version => "version",
            Type::Uuid => "uuid",
            Type::Nanoid => "nanoid",
            Type::List(_) => "list",
        }
    }

    /// The full name as written in a `type:`, such as `"list"` or `"list(string)"`.
    pub fn name(&self) -> Cow<'static, str> {
        match self {
            Type::String => Cow::Borrowed("string"),
            Type::Int => Cow::Borrowed("int"),
            Type::Uint => Cow::Borrowed("uint"),
            Type::Float => Cow::Borrowed("float"),
            Type::Bool => Cow::Borrowed("bool"),
            Type::Datetime => Cow::Borrowed("datetime"),
            Type::Version => Cow::Borrowed("version"),
            Type::Uuid => Cow::Borrowed("uuid"),
            Type::Nanoid => Cow::Borrowed("nanoid"),
            Type::List(None) => Cow::Borrowed("list"),
            Type::List(Some(inner)) => Cow::Owned(format!("list({})", inner)),
        }
    }

    /// The base type a name stands for.
    pub fn from_name(name: &str) -> Option<Type> {
        Type::ALL.iter().find(|ty| ty.base_name() == name).cloned()
    }

    /// Reads a type specification from text (e.g. `"string"`, `"list"`, or `"list(string)"`).
    pub fn parse_str(name: &str) -> Result<Type, TypeError> {
        let trimmed = name.trim();
        if let Some(inner) = trimmed
            .strip_prefix("list(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let inner_trimmed = inner.trim();
            if inner_trimmed.is_empty() {
                return Ok(Type::List(None));
            }
            let inner_type = Type::parse_str(inner_trimmed)?;
            return Ok(Type::List(Some(Box::new(inner_type))));
        }
        Type::from_name(trimmed).ok_or_else(|| TypeError::Unknown(name.to_string()))
    }

    /// What it is, for a sentence: `an integer` or `a list of strings`.
    pub fn describe(&self) -> String {
        match self {
            Type::String => "a string".to_string(),
            Type::Int => "an integer".to_string(),
            Type::Uint => "a non-negative integer".to_string(),
            Type::Float => "a number".to_string(),
            Type::Bool => "`true` or `false`".to_string(),
            Type::Datetime => "a date or a datetime (RFC 3339)".to_string(),
            Type::Version => "a version such as `1.10.0`".to_string(),
            Type::Uuid => "a UUID".to_string(),
            Type::Nanoid => "an identifier".to_string(),
            Type::List(None) => "a list".to_string(),
            Type::List(Some(inner)) => format!("a list of {}", inner.describe()),
        }
    }

    /// Whether this type gives text a form of its own to be held to: `datetime`,
    /// `version` and `uuid`. [`holds_text`](Type::holds_text) is `Some` for
    /// exactly these.
    pub fn has_text_form(&self) -> bool {
        matches!(self, Type::Datetime | Type::Version | Type::Uuid)
    }

    /// Whether `text` is in the form this type gives text, or `None` when the
    /// type gives text no form of its own.
    pub fn holds_text(&self, text: &str) -> Option<bool> {
        match self {
            Type::Datetime => Some(datetime::parse_datetime_parts(text).is_some()),
            Type::Version => Some(Version::parse(text.trim()).is_some()),
            Type::Uuid => Some(is_uuid(text)),
            _ => None,
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::String => f.write_str("string"),
            Type::Int => f.write_str("int"),
            Type::Uint => f.write_str("uint"),
            Type::Float => f.write_str("float"),
            Type::Bool => f.write_str("bool"),
            Type::Datetime => f.write_str("datetime"),
            Type::Version => f.write_str("version"),
            Type::Uuid => f.write_str("uuid"),
            Type::Nanoid => f.write_str("nanoid"),
            Type::List(None) => f.write_str("list"),
            Type::List(Some(inner)) => write!(f, "list({inner})"),
        }
    }
}

/// A `type:` that does not name a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    /// The value was not a bare name -- it was a call, a list, or another structure.
    NotAName,
    /// The name does not match any known type.
    Unknown(String),
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names = Type::ALL.map(|ty| format!("`{ty}`")).join(", ");
        match self {
            TypeError::Unknown(name) => {
                write!(f, "`{name}` is not a type; the types are {names}")
            }
            TypeError::NotAName => write!(
                f,
                "a type is written as a name, one of {names}; a type with a parameter has no spelling yet"
            ),
        }
    }
}

impl std::error::Error for TypeError {}

/// The 8-4-4-4-12 hexadecimal form, in either case. The version and variant are
/// not looked at: what a `uuid` field asks for is an identifier of this shape,
/// not a particular way of drawing one.
pub fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_has_a_name_roundtrip() {
        for ty in Type::ALL {
            assert_eq!(Type::from_name(ty.base_name()), Some(ty.clone()));
            assert_eq!(Type::parse_str(&ty.name()), Ok(ty));
        }
        assert_eq!(
            Type::parse_str("list(string)"),
            Ok(Type::List(Some(Box::new(Type::String))))
        );
        assert_eq!(
            Type::parse_str("list(list(int))"),
            Ok(Type::List(Some(Box::new(Type::List(Some(Box::new(
                Type::Int
            )))))))
        );
        assert_eq!(
            Type::parse_str("list(string)").unwrap().name(),
            "list(string)"
        );
    }

    #[test]
    fn parse_unknown_name() {
        assert_eq!(
            Type::parse_str("date"),
            Err(TypeError::Unknown("date".to_string()))
        );
    }

    #[test]
    fn holds_text_matches_intended_forms() {
        assert_eq!(Type::Datetime.holds_text("2026-10-25"), Some(true));
        assert_eq!(Type::Datetime.holds_text("not a date"), Some(false));
        assert_eq!(Type::Version.holds_text("1.10.0"), Some(true));
        assert_eq!(Type::Version.holds_text("bad"), Some(false));
        assert_eq!(
            Type::Uuid.holds_text("123e4567-e89b-12d3-a456-426614174000"),
            Some(true)
        );
        assert_eq!(Type::Uuid.holds_text("not-a-uuid"), Some(false));
        assert_eq!(Type::String.holds_text("anything"), None);
    }
}
