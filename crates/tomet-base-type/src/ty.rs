//! [`Type`]: a kind and the arguments that kind takes.

use std::fmt;
use std::str::FromStr;

use crate::{AutoFill, DefaultValue, Kind, TypeError, Version, datetime, is_uuid, text};

/// A type a value can be declared as: a [`Kind`] with the arguments that kind
/// takes, and a `default`.
///
/// - `uuid` has a `prefix`;
/// - `nanoid` has a `length` and a `prefix`;
/// - `list` has an item type and a `length`;
/// - every kind has a `default`.
///
/// What a kind takes is part of the kind, so an argument that does not belong
/// to it (a `string` with a `prefix`) cannot be held: a type is made by
/// [`Type::build`] from what was written, or by [`Type::bare`] with nothing
/// written, and nowhere else. Its text form -- [`Display`](fmt::Display) and
/// [`FromStr`] -- is `string`, `list(string, length: 6)`,
/// `nanoid(length: 8, prefix: "doc-")`, `uuid(default: auto)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub(crate) form: Form,
    pub(crate) default: Option<DefaultValue>,
}

// A type holds no number that is not equal to itself: `Type::build` is the only way to make one
// that holds a number, and it refuses `NaN`.
impl Eq for Type {}

/// The kind of a type with the arguments only that kind has.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Form {
    String,
    Int,
    Uint,
    Float,
    Bool,
    Datetime,
    Date,
    Version,
    Uuid {
        prefix: Option<String>,
    },
    Nanoid {
        length: Option<usize>,
        prefix: Option<String>,
    },
    List {
        item: Option<Box<Type>>,
        length: Option<usize>,
    },
}

impl Type {
    /// The type of `kind` with no arguments written: `string`, `list`, `uuid`.
    pub fn bare(kind: Kind) -> Type {
        let form = match kind {
            Kind::String => Form::String,
            Kind::Int => Form::Int,
            Kind::Uint => Form::Uint,
            Kind::Float => Form::Float,
            Kind::Bool => Form::Bool,
            Kind::Datetime => Form::Datetime,
            Kind::Date => Form::Date,
            Kind::Version => Form::Version,
            Kind::Uuid => Form::Uuid { prefix: None },
            Kind::Nanoid => Form::Nanoid {
                length: None,
                prefix: None,
            },
            Kind::List => Form::List {
                item: None,
                length: None,
            },
        };
        Type {
            form,
            default: None,
        }
    }

    /// Which kind of type it is.
    pub fn kind(&self) -> Kind {
        match self.form {
            Form::String => Kind::String,
            Form::Int => Kind::Int,
            Form::Uint => Kind::Uint,
            Form::Float => Kind::Float,
            Form::Bool => Kind::Bool,
            Form::Datetime => Kind::Datetime,
            Form::Date => Kind::Date,
            Form::Version => Kind::Version,
            Form::Uuid { .. } => Kind::Uuid,
            Form::Nanoid { .. } => Kind::Nanoid,
            Form::List { .. } => Kind::List,
        }
    }

    /// The type of a list's items; `None` for a list that does not say, and for
    /// every kind but `list`.
    pub fn item(&self) -> Option<&Type> {
        match &self.form {
            Form::List { item, .. } => item.as_deref(),
            _ => None,
        }
    }

    /// The `length` of a `nanoid` (how many characters its body has) or of a
    /// `list` (how many items); `None` when it is not given, and for every other
    /// kind.
    pub fn length(&self) -> Option<usize> {
        match self.form {
            Form::Nanoid { length, .. } | Form::List { length, .. } => length,
            _ => None,
        }
    }

    /// The `prefix` of a `uuid` or a `nanoid`: the text before the body of the
    /// id. `None` when there is none -- an empty prefix is none -- and for every
    /// other kind.
    pub fn prefix(&self) -> Option<&str> {
        match &self.form {
            Form::Uuid { prefix } | Form::Nanoid { prefix, .. } => prefix.as_deref(),
            _ => None,
        }
    }

    /// What a value of this type is when none is written.
    pub fn default(&self) -> Option<&DefaultValue> {
        self.default.as_ref()
    }

    /// What its `default: auto` makes up, when its default is `auto`.
    ///
    /// A type whose default is another value, or none, is `None`; so is every
    /// kind that has nothing to make up, because it refuses `auto` when it is
    /// built. [`Kind::auto`] is the same question asked of the kind alone.
    pub fn auto(&self) -> Option<AutoFill> {
        match self.default {
            Some(DefaultValue::Auto) => self.kind().auto(),
            _ => None,
        }
    }

    /// What it is, for a sentence: `an integer` or `a list of a string`.
    pub fn describe(&self) -> String {
        match &self.form {
            Form::String => "a string".to_string(),
            Form::Int => "an integer".to_string(),
            Form::Uint => "a non-negative integer".to_string(),
            Form::Float => "a number".to_string(),
            Form::Bool => "`true` or `false`".to_string(),
            Form::Datetime => "a date or a datetime (RFC 3339)".to_string(),
            Form::Date => "a date such as `2026-10-25`".to_string(),
            Form::Version => "a version such as `1.10.0`".to_string(),
            Form::Uuid { prefix: None } => "a UUID".to_string(),
            Form::Uuid {
                prefix: Some(prefix),
            } => format!("a UUID after the prefix `{prefix}`"),
            Form::Nanoid { .. } => "an identifier".to_string(),
            Form::List { item, length } => {
                let mut what = match length {
                    Some(1) => "a list of 1 item".to_string(),
                    Some(n) => format!("a list of {n} items"),
                    None => "a list".to_string(),
                };
                if let Some(item) = item {
                    let each = if length.is_some() { ", each" } else { " of" };
                    what.push_str(&format!("{each} {}", item.describe()));
                }
                what
            }
        }
    }

    /// Whether this type gives text a form of its own to be held to: `datetime`,
    /// `date`, `version` and `uuid`. [`holds_text`](Type::holds_text) is `Some`
    /// for exactly these.
    pub fn has_text_form(&self) -> bool {
        matches!(
            self.form,
            Form::Datetime | Form::Date | Form::Version | Form::Uuid { .. }
        )
    }

    /// Whether `text` is in the form this type gives text, or `None` when the
    /// type gives text no form of its own.
    ///
    /// A `uuid` with a `prefix` is the prefix and then the UUID. A `nanoid` has
    /// no form here, whatever its `length` and `prefix`: any text is one. They
    /// say how an id is drawn and what shape a vault's ids are held to, which
    /// `tomet-id` reads from the type; they do not narrow what the type admits.
    pub fn holds_text(&self, text: &str) -> Option<bool> {
        match &self.form {
            Form::Datetime => Some(datetime::parse_datetime_parts(text).is_some()),
            Form::Date => Some(datetime::is_date(text)),
            Form::Version => Some(Version::parse(text.trim()).is_some()),
            Form::Uuid { prefix } => Some(
                text.strip_prefix(prefix.as_deref().unwrap_or(""))
                    .is_some_and(is_uuid),
            ),
            _ => None,
        }
    }
}

/// The text form: the kind's name, then its arguments in parentheses when it
/// has any -- the item type first, then `length`, `prefix` and `default`.
/// [`FromStr`] reads exactly what this writes.
impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut arguments: Vec<String> = Vec::new();
        match &self.form {
            Form::Uuid {
                prefix: Some(prefix),
            } => arguments.push(format!("prefix: {}", text::quoted(prefix))),
            Form::Nanoid { length, prefix } => {
                if let Some(length) = length {
                    arguments.push(format!("length: {length}"));
                }
                if let Some(prefix) = prefix {
                    arguments.push(format!("prefix: {}", text::quoted(prefix)));
                }
            }
            Form::List { item, length } => {
                if let Some(item) = item {
                    arguments.push(item.to_string());
                }
                if let Some(length) = length {
                    arguments.push(format!("length: {length}"));
                }
            }
            _ => {}
        }
        if let Some(default) = &self.default {
            arguments.push(format!("default: {default}"));
        }
        f.write_str(self.kind().name())?;
        if !arguments.is_empty() {
            write!(f, "({})", arguments.join(", "))?;
        }
        Ok(())
    }
}

/// Reads the text form: a name, with arguments in parentheses when the type
/// takes them. The same rules as [`Type::build`] apply to what is read.
impl FromStr for Type {
    type Err = TypeError;

    fn from_str(text: &str) -> Result<Type, TypeError> {
        text::parse(text)
    }
}
