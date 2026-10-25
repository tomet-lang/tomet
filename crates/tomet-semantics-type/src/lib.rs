//! =[ tomet-type ]
//!
//! The types a value can be declared as, and how data and text are held to
//! them.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (One vocabulary)
//!   | A `type:` is written in the settings' `meta` fields, in `::vars`, and in
//!   | a vocabulary's arguments. They all name a [`Type`] here, so a name that
//!   | one place accepts is a name every place accepts.
//! - (Knows nothing about elements)
//!   | This crate depends on [`tomet_ast::Value`] and nothing else. A type does
//!   | not know what an element is, which vocabulary declared it, or what
//!   | `::meta` is; the dependency is what keeps it so.
//! - (What is held to a type)
//!   | [`Type::admits`] asks whether a [`Value`] in a document is of the type.
//!   | [`Type::read_text`] turns a text that arrives as text -- `--var key=value` --
//!   | into the value of the type. [`Type::holds_text`] asks whether a string
//!   | is in the form the type gives text, for the types that have one.
//!
//! =[ What a type is not ]
//!
//! - It is not how a field is written or made. `format`, `offset`, `length`,
//!   `prefix` and `always_newline` are properties of a field's declaration, and
//!   belong to the formatter. In particular what makes a `nanoid` well formed
//!   depends on the field's `prefix` and `length`, so [`Type::Nanoid`] admits a
//!   string and judges no more; `tomet-format-field-utils` judges the rest.
//! - It is not a value in the evaluator. A datetime read here stays text, and
//!   [`datetime`] gives its parts; `tomet-expr` turns it into its own value.
//!
//! =[ Not yet ]
//!
//! - A version with a pre-release or build part (`1.0.0-beta`, `+5`).
//! - A type with a parameter. `list` has no element type yet; the author expects
//!   it to be written `<list<string>>` when it comes, so [`Type::parse`] reads
//!   a name and nothing else, and says so for any other spelling.
//! - `enum`. Nothing gives it a meaning yet (`:rule`'s `allow:` takes the same
//!   spelling and means a list of names).

use tomet_ast::Value;

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

    /// Reads a `type:` as it is written in a document.
    ///
    /// Only a name is read. Any other spelling -- a call such as `list(string)`,
    /// a list, a map -- is a [`TypeError::NotAName`], because a type with a
    /// parameter has no spelling yet.
    pub fn parse(spec: &Value) -> Result<Type, TypeError> {
        match spec {
            Value::String(name) => {
                Type::from_name(name.trim()).ok_or_else(|| TypeError::Unknown(name.clone()))
            }
            _ => Err(TypeError::NotAName),
        }
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
    ///
    /// Only `datetime`, `version` and `uuid` have one. A `string` is any text,
    /// and what makes a `nanoid` well formed is the field's `prefix` and
    /// `length`. Surrounding whitespace is not looked at, as for a datetime.
    pub fn holds_text(self, text: &str) -> Option<bool> {
        match self {
            Type::Datetime => Some(datetime::parse_datetime_parts(text).is_some()),
            Type::Version => Some(Version::parse(text.trim()).is_some()),
            Type::Uuid => Some(is_uuid(text)),
            _ => None,
        }
    }

    /// Whether `value`, as a document holds it, is of this type.
    pub fn admits(self, value: &Value) -> Result<(), Mismatch> {
        let fits = match (self, value) {
            (Type::String | Type::Nanoid, Value::String(_)) => true,
            (Type::Int, Value::Int(_)) => true,
            (Type::Uint, Value::Int(n)) => *n >= 0,
            (Type::Float, Value::Float(_) | Value::Int(_)) => true,
            (Type::Bool, Value::Bool(_)) => true,
            (Type::Datetime | Type::Uuid, Value::String(text)) => {
                self.holds_text(text) == Some(true)
            }
            (Type::List, Value::Seq(_)) => true,
            // A version is text, and a bare `1`, `1.0` or `1.10` is the text it was
            // written as: what is written decides, not what it would be as a number.
            (Type::Version, scalar) => scalar
                .scalar_text()
                .is_some_and(|text| self.holds_text(&text) == Some(true)),
            _ => false,
        };
        if fits {
            Ok(())
        } else {
            Err(Mismatch {
                ty: self,
                found: describe_value(value),
            })
        }
    }

    /// The value of this type that `text` stands for, when the text arrives as
    /// text and nothing else: `tomet new --var n=3`.
    ///
    /// A datetime, a version or a UUID stays the text it was, trimmed, once it is
    /// known to be well formed. A `list` has no single-text form.
    pub fn read_text(self, text: &str) -> Result<Value, TextError> {
        let found = || format!("`{text}`");
        let mismatch = |ty: Type| TextError::Mismatch(Mismatch { ty, found: found() });
        match self {
            Type::String | Type::Nanoid => Ok(Value::String(text.to_string())),
            Type::Int => text
                .trim()
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|_| mismatch(self)),
            Type::Uint => match text.trim().parse::<i64>() {
                Ok(n) if n >= 0 => Ok(Value::Int(n)),
                _ => Err(mismatch(self)),
            },
            Type::Float => match text.trim().parse::<f64>() {
                Ok(n) if n.is_finite() => Ok(Value::Float(n)),
                _ => Err(mismatch(self)),
            },
            Type::Bool => match text.trim() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => Err(mismatch(self)),
            },
            Type::Datetime | Type::Version | Type::Uuid => {
                let text = text.trim();
                if self.holds_text(text) == Some(true) {
                    Ok(Value::String(text.to_string()))
                } else {
                    Err(mismatch(self))
                }
            }
            Type::List => Err(TextError::NotText(self)),
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// The 8-4-4-4-12 hexadecimal form, in either case. The version and variant are
/// not looked at: what a `uuid` field asks for is an identifier of this shape,
/// not a particular way of drawing one.
fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// A value as a reader would have written it, for a message.
fn describe_value(value: &Value) -> String {
    match value {
        Value::String(text) => format!("`{text}`"),
        Value::Bool(b) => format!("`{b}`"),
        Value::Int(n) => format!("`{n}`"),
        Value::Float(n) => format!("`{n}`"),
        Value::Null => "null".to_string(),
        Value::Seq(_) => "a list".to_string(),
        Value::Map(_) => "a map".to_string(),
        _ => "something else".to_string(),
    }
}

/// A `type:` that does not name a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    /// A name that is not one of [`Type::ALL`].
    Unknown(String),
    /// Not a plain name: a call, a list, a map. A type with a parameter has no
    /// spelling yet.
    NotAName,
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

/// A value that is not of the type it was held to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub ty: Type,
    /// What was found, as a reader would write it: `` `many` ``, `a map`.
    pub found: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not {}", self.found, self.ty.describe())
    }
}

impl std::error::Error for Mismatch {}

/// Why a text is not a value of a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextError {
    /// The text is not of the type.
    Mismatch(Mismatch),
    /// The type has no form as a single text, so no text can be one: a `list`.
    NotText(Type),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::Mismatch(mismatch) => mismatch.fmt(f),
            TextError::NotText(ty) => {
                write!(
                    f,
                    "{} cannot be written as a single text yet",
                    ty.describe()
                )
            }
        }
    }
}

impl std::error::Error for TextError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Value {
        Value::String(s.to_string())
    }

    #[test]
    fn every_type_is_found_by_the_name_it_is_written_as() {
        for ty in Type::ALL {
            assert_eq!(Type::from_name(ty.name()), Some(ty), "{ty}");
            assert_eq!(Type::parse(&text(ty.name())), Ok(ty), "{ty}");
        }
        assert_eq!(Type::ALL.len(), 10);
    }

    #[test]
    fn a_type_is_a_name_and_the_error_says_which_names_there_are() {
        let err = Type::parse(&text("date")).unwrap_err();
        assert_eq!(err, TypeError::Unknown("date".to_string()));
        assert!(err.to_string().contains("`datetime`"), "{err}");
        // a call, a list and a map are not names: a type with a parameter has no spelling yet
        for spec in [
            Value::Call("list".to_string(), vec![text("string")]),
            Value::Seq(vec![text("string")]),
            Value::Map(vec![]),
            Value::Int(3),
        ] {
            assert_eq!(Type::parse(&spec), Err(TypeError::NotAName), "{spec:?}");
        }
    }

    #[test]
    fn a_value_is_held_to_its_type() {
        let ok: [(Type, Value); 17] = [
            (Type::String, text("x")),
            (Type::Int, Value::Int(-3)),
            (Type::Uint, Value::Int(0)),
            (Type::Float, Value::Float(1.5)),
            (Type::Float, Value::Int(2)),
            (Type::Bool, Value::Bool(false)),
            (Type::Datetime, text("2026-10-25")),
            (Type::Datetime, text("2026-10-25T09:00:00+09:00")),
            (Type::Version, text("1.10.0")),
            (Type::Version, text("1.0")),
            // bare, a version is the text it was written as
            (Type::Version, Value::Int(1)),
            (Type::Version, Value::Float(1.5)),
            (Type::Version, Value::Number("1.10".to_string())),
            (Type::Uuid, text("123e4567-e89b-12d3-a456-426614174000")),
            (Type::Nanoid, text("kWBcIUUz")),
            (Type::List, Value::Seq(vec![])),
            (Type::List, Value::Seq(vec![text("a")])),
        ];
        for (ty, value) in ok {
            assert_eq!(ty.admits(&value), Ok(()), "{ty}: {value:?}");
        }
        let bad: [(Type, Value); 14] = [
            (Type::String, Value::Int(1)),
            (Type::Int, Value::Float(1.0)),
            (Type::Uint, Value::Int(-1)),
            (Type::Bool, text("true")),
            (Type::Datetime, text("tomorrow")),
            (Type::Datetime, Value::Int(2026)),
            (Type::Version, text("1.0-beta")),
            (Type::Version, text("v1")),
            (Type::Version, Value::Bool(true)),
            (Type::Version, Value::Null),
            (Type::Version, Value::Float(1e21)),
            (Type::Uuid, text("not-a-uuid")),
            (Type::List, text("a")),
            (Type::Nanoid, Value::Int(1)),
        ];
        for (ty, value) in bad {
            assert!(ty.admits(&value).is_err(), "{ty}: {value:?}");
        }
    }

    #[test]
    fn a_mismatch_says_what_was_found_and_what_was_wanted() {
        let err = Type::Int.admits(&text("many")).unwrap_err();
        assert_eq!(err.to_string(), "`many` is not an integer");
        let err = Type::String.admits(&Value::Map(vec![])).unwrap_err();
        assert_eq!(err.to_string(), "a map is not a string");
    }

    #[test]
    fn a_text_becomes_the_value_of_the_type() {
        assert_eq!(Type::String.read_text(" x "), Ok(text(" x ")));
        assert_eq!(Type::Int.read_text(" 41 "), Ok(Value::Int(41)));
        assert_eq!(Type::Uint.read_text("0"), Ok(Value::Int(0)));
        assert_eq!(Type::Float.read_text("1.5"), Ok(Value::Float(1.5)));
        assert_eq!(Type::Bool.read_text("true"), Ok(Value::Bool(true)));
        assert_eq!(
            Type::Datetime.read_text(" 2026-10-25 "),
            Ok(text("2026-10-25"))
        );
        assert_eq!(Type::Version.read_text(" 1.10.0 "), Ok(text("1.10.0")));
        assert_eq!(
            Type::Uuid.read_text("123E4567-E89B-12D3-A456-426614174000"),
            Ok(text("123E4567-E89B-12D3-A456-426614174000"))
        );
    }

    #[test]
    fn a_text_that_is_not_of_the_type_is_refused_by_name() {
        for (ty, bad) in [
            (Type::Int, "three"),
            (Type::Uint, "-1"),
            (Type::Float, "NaN"),
            (Type::Float, "inf"),
            (Type::Bool, "yes"),
            (Type::Datetime, "tomorrow"),
            (Type::Version, "1.0-beta"),
            (Type::Uuid, "123"),
        ] {
            let err = ty.read_text(bad).unwrap_err();
            assert!(
                matches!(err, TextError::Mismatch(ref m) if m.ty == ty),
                "{ty} {bad}: {err}"
            );
        }
    }

    #[test]
    fn a_list_has_no_single_text_form() {
        assert_eq!(
            Type::List.read_text("a,b"),
            Err(TextError::NotText(Type::List))
        );
    }

    #[test]
    fn only_datetime_version_and_uuid_have_a_form_for_text() {
        assert_eq!(Type::Version.holds_text("1.10.0"), Some(true));
        assert_eq!(Type::Version.holds_text(" 1.10 "), Some(true));
        assert_eq!(Type::Version.holds_text("1.10-rc"), Some(false));
        assert_eq!(Type::Datetime.holds_text("2026-10-25"), Some(true));
        assert_eq!(Type::Datetime.holds_text("soon"), Some(false));
        assert_eq!(
            Type::Uuid.holds_text("123e4567-e89b-12d3-a456-426614174000"),
            Some(true)
        );
        assert_eq!(Type::Uuid.holds_text("123e4567"), Some(false));
        for ty in Type::ALL {
            assert_eq!(
                ty.holds_text("anything").is_some(),
                ty.has_text_form(),
                "{ty}"
            );
        }
        for ty in [Type::String, Type::Nanoid, Type::Int, Type::List] {
            assert_eq!(ty.holds_text("anything"), None, "{ty}");
        }
    }
}
