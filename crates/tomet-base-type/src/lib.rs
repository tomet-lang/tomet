//! =[ tomet-type ]
//!
//! The types a value can be declared as, and how text is held to them.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (One vocabulary)
//!   | A `type:` is written in the settings' `meta` fields, in `::vars`, and in
//!   | a vocabulary's arguments. They all name a [`Type`] here, so a name that
//!   | one place accepts is a name every place accepts.
//! - (A pure base-layer crate)
//!   | This crate is in the base layer (`crates/tomet-base-*`) and depends on
//!   | nothing in the workspace. A type does not know what an element is, what
//!   | an AST node is, or which vocabulary declared it.
//! - (What is held to a type)
//!   | [`Type::holds_text`] asks whether a string is in the form the type gives
//!   | text, for the types that have one.

pub mod datetime;
mod version;

pub use version::Version;

/// A type a value can be declared as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    /// A version: dotted non-negative integers, `1`, `1.0`, `1.10.0`
    /// ([`Version`]). It is text. A bare `1.10` is read as the text it was
    /// written as, not as the number it would be.
    Version,
    /// An identifier in the 8-4-4-4-12 hexadecimal form.
    Uuid,
    /// A short generated identifier. Its form depends on the field's `prefix`
    /// and `length`, which are not the type's; this admits any string.
    Nanoid,
    /// A list of values. It has no element type yet.
    List,
}

impl Type {
    /// Every type, in the order they are listed to people.
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
        Type::List,
    ];

    /// The name it is written as in a `type:`.
    pub fn name(self) -> &'static str {
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
            Type::List => "list",
        }
    }

    /// The type a name stands for.
    pub fn from_name(name: &str) -> Option<Type> {
        Type::ALL.into_iter().find(|ty| ty.name() == name)
    }

    /// Reads a type name from text.
    pub fn parse_str(name: &str) -> Result<Type, TypeError> {
        let trimmed = name.trim();
        Type::from_name(trimmed).ok_or_else(|| TypeError::Unknown(name.to_string()))
    }

    /// What it is, for a sentence: `an integer`.
    pub fn describe(self) -> &'static str {
        match self {
            Type::String => "a string",
            Type::Int => "an integer",
            Type::Uint => "a non-negative integer",
            Type::Float => "a number",
            Type::Bool => "`true` or `false`",
            Type::Datetime => "a date or a datetime (RFC 3339)",
            Type::Version => "a version such as `1.10.0`",
            Type::Uuid => "a UUID",
            Type::Nanoid => "an identifier",
            Type::List => "a list",
        }
    }

    /// Whether this type gives text a form of its own to be held to: `datetime`,
    /// `version` and `uuid`. [`holds_text`](Type::holds_text) is `Some` for
    /// exactly these.
    pub fn has_text_form(self) -> bool {
        matches!(self, Type::Datetime | Type::Version | Type::Uuid)
    }

    /// Whether `text` is in the form this type gives text, or `None` when the
    /// type gives text no form of its own.
    pub fn holds_text(self, text: &str) -> Option<bool> {
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
        f.write_str(self.name())
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
            assert_eq!(Type::from_name(ty.name()), Some(ty));
            assert_eq!(Type::parse_str(ty.name()), Ok(ty));
        }
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
