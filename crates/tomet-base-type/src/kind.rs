//! The kinds of type, and what each one takes as arguments.

/// Which type it is, apart from its arguments: the name a type is written with.
///
/// A [`Type`](crate::Type) is a kind and the arguments that kind takes
/// (`nanoid` a `length` and a `prefix`, `list` an item type and a `length`),
/// so `list(string)` and `list` are the same kind and not the same type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
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
    /// A date, or a date and a time ([`datetime::parse_datetime_parts`](crate::datetime::parse_datetime_parts)).
    Datetime,
    /// A calendar date, `YYYY-MM-DD` ([`datetime::is_date`](crate::datetime::is_date)).
    Date,
    /// A version: dotted non-negative integers (`1`, `1.0`, `1.10.0`), with
    /// optional pre-release (`-beta`) and build metadata (`+5`)
    /// ([`Version`](crate::Version)). It is text. A bare `1.10` is read as the
    /// text it was written as, not as the number it would be.
    Version,
    /// An identifier in the 8-4-4-4-12 hexadecimal form.
    Uuid,
    /// A short generated identifier.
    Nanoid,
    /// A list of values.
    List,
}

/// What a `default: auto` asks for: a value the type makes up when a value is
/// needed and none was written.
///
/// This crate says which kinds have one ([`Kind::auto`]) and of what sort; it
/// draws and reads nothing itself, so it has neither a random source nor a
/// clock. Whoever fills in the value owns both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AutoFill {
    /// A newly drawn identifier of the type's shape (`uuid`, `nanoid`).
    Id,
    /// The moment it is made (`datetime`, `date`).
    Now,
}

impl Kind {
    /// Every kind, in the order they are listed to people.
    pub const ALL: [Kind; 11] = [
        Kind::String,
        Kind::Int,
        Kind::Uint,
        Kind::Float,
        Kind::Bool,
        Kind::Datetime,
        Kind::Date,
        Kind::Version,
        Kind::Uuid,
        Kind::Nanoid,
        Kind::List,
    ];

    /// The name it is written with: `"list"` for any list.
    pub fn name(self) -> &'static str {
        match self {
            Kind::String => "string",
            Kind::Int => "int",
            Kind::Uint => "uint",
            Kind::Float => "float",
            Kind::Bool => "bool",
            Kind::Datetime => "datetime",
            Kind::Date => "date",
            Kind::Version => "version",
            Kind::Uuid => "uuid",
            Kind::Nanoid => "nanoid",
            Kind::List => "list",
        }
    }

    /// The kind a name stands for.
    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// What `default: auto` means for this kind, or `None` when the kind has no
    /// value to make up and so refuses it.
    pub fn auto(self) -> Option<AutoFill> {
        match self {
            Kind::Uuid | Kind::Nanoid => Some(AutoFill::Id),
            Kind::Datetime | Kind::Date => Some(AutoFill::Now),
            _ => None,
        }
    }

    /// How many arguments are written by position. A `list` has one, its item
    /// type; no other kind has any.
    pub(crate) fn positional(self) -> usize {
        match self {
            Kind::List => 1,
            _ => 0,
        }
    }

    /// The names of the arguments written as `name: value`, in the order they
    /// are written; `default` is last and every kind has it.
    pub(crate) fn named(self) -> &'static [&'static str] {
        match self {
            Kind::Uuid => &["prefix", "default"],
            Kind::Nanoid => &["length", "prefix", "default"],
            Kind::List => &["length", "default"],
            _ => &["default"],
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_is_found_by_its_name() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(Kind::from_name("person"), None);
        assert_eq!(Kind::from_name("List"), None);
        assert_eq!(Kind::from_name("list(string)"), None);
    }

    #[test]
    fn only_the_kinds_that_make_a_value_up_have_an_auto() {
        let with: Vec<_> = Kind::ALL
            .into_iter()
            .filter_map(|kind| kind.auto().map(|fill| (kind.name(), fill)))
            .collect();
        assert_eq!(
            with,
            [
                ("datetime", AutoFill::Now),
                ("date", AutoFill::Now),
                ("uuid", AutoFill::Id),
                ("nanoid", AutoFill::Id),
            ]
        );
    }

    #[test]
    fn every_kind_has_a_default_and_it_is_written_last() {
        for kind in Kind::ALL {
            assert_eq!(kind.named().last(), Some(&"default"), "{kind}");
        }
    }
}
