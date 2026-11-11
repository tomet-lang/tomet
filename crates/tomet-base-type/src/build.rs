//! Building a type from what was written for it, and the rules that decide
//! whether it is one.

use crate::ty::Form;
use crate::{Arg, Datum, DefaultValue, Kind, Type, TypeError};

impl Type {
    /// Builds a type from a name and the arguments written for it: the ones
    /// written by position, in order, and the ones written as `name: value`.
    ///
    /// This is the one place that decides what a type may be given, so that
    /// every way of writing one -- a document, a setting, the text form, an
    /// expression -- is held to the same rules:
    ///
    /// - the name is a [`Kind`];
    /// - a `list` takes its item type by position, as a type without a
    ///   `default` of its own; no other kind takes anything by position;
    /// - `length` (`nanoid`, `list`) is a whole number, at least 1 for a
    ///   `nanoid` and not negative for a `list`;
    /// - `prefix` (`uuid`, `nanoid`) is text, and an empty prefix is none;
    /// - `default` (every kind) is a plain value, or `auto` for a kind that
    ///   makes a value up ([`Kind::auto`]). It is not held to the type here
    ///   (see [`DefaultValue::Given`]);
    /// - `overwrite` (every kind) is a boolean (`true` or `false`), defaulting to `false`;
    /// - a name that is not one of these, or is written twice, is an error;
    /// - [`Arg::Auto`] is the value of `default` and of no other argument.
    pub fn build(name: &str, positional: &[Arg], named: &[(&str, Arg)]) -> Result<Type, TypeError> {
        let kind = Kind::from_name(name).ok_or_else(|| TypeError::Unknown(name.to_string()))?;

        if positional.len() > kind.positional() {
            return Err(TypeError::TooManyArguments {
                ty: kind,
                takes: kind.positional(),
                given: positional.len(),
            });
        }
        let mut seen: Vec<&str> = Vec::new();
        for (key, _) in named {
            if !kind.named().contains(key) {
                return Err(TypeError::UnknownArgument {
                    ty: kind,
                    argument: key.to_string(),
                });
            }
            if seen.contains(key) {
                return Err(TypeError::DuplicateArgument {
                    ty: kind,
                    argument: key.to_string(),
                });
            }
            seen.push(key);
        }
        let get = |key: &str| named.iter().find(|(k, _)| *k == key).map(|(_, arg)| arg);

        // `auto` has one place; anything else is refused before the
        // parameters are read, so that it is not taken for a wrong value.
        if let Some(Arg::Auto) = positional.first() {
            return Err(TypeError::AutoElsewhere {
                ty: kind,
                argument: "item".to_string(),
            });
        }
        for (key, arg) in named {
            if *key != "default" && matches!(arg, Arg::Auto) {
                return Err(TypeError::AutoElsewhere {
                    ty: kind,
                    argument: key.to_string(),
                });
            }
        }

        let default = get("default")
            .map(|arg| default_of(kind, arg))
            .transpose()?;
        let overwrite = get("overwrite")
            .map(|arg| overwrite_of(kind, arg))
            .transpose()?
            .unwrap_or(false);
        let form = match kind {
            Kind::String => Form::String,
            Kind::Int => Form::Int,
            Kind::Uint => Form::Uint,
            Kind::Float => Form::Float,
            Kind::Bool => Form::Bool,
            Kind::Datetime => Form::Datetime,
            Kind::Date => Form::Date,
            Kind::Version => Form::Version,
            Kind::Uuid => Form::Uuid {
                prefix: get("prefix")
                    .map(|arg| prefix_of(kind, arg))
                    .transpose()?
                    .flatten(),
            },
            Kind::Nanoid => Form::Nanoid {
                length: get("length")
                    .map(|arg| length_of(kind, arg, 1))
                    .transpose()?,
                prefix: get("prefix")
                    .map(|arg| prefix_of(kind, arg))
                    .transpose()?
                    .flatten(),
            },
            Kind::List => Form::List {
                item: positional
                    .first()
                    .map(|arg| item_of(kind, arg))
                    .transpose()?
                    .map(Box::new),
                length: get("length")
                    .map(|arg| length_of(kind, arg, 0))
                    .transpose()?,
            },
        };
        Ok(Type {
            form,
            default,
            overwrite,
        })
    }
}

fn wrong(kind: Kind, argument: &'static str, expected: &str, found: &Arg) -> TypeError {
    TypeError::WrongArgument {
        ty: kind,
        argument,
        expected: expected.to_string(),
        found: found.describe(),
    }
}

/// A list's item type: a type that has no `default` of its own, since a value
/// is a list's to default and not each item's.
fn item_of(kind: Kind, arg: &Arg) -> Result<Type, TypeError> {
    match arg {
        Arg::Type(item) if item.default.is_none() => Ok(item.clone()),
        Arg::Type(_) => Err(wrong(
            kind,
            "item",
            "a type without a `default` (the default is the list's)",
            arg,
        )),
        _ => Err(wrong(kind, "item", "a type", arg)),
    }
}

/// A `length`: a whole number of at least `least`.
fn length_of(kind: Kind, arg: &Arg, least: usize) -> Result<usize, TypeError> {
    match arg {
        Arg::Datum(Datum::Int(n)) if usize::try_from(*n).is_ok_and(|n| n >= least) => {
            Ok(usize::try_from(*n).unwrap_or(least))
        }
        _ => Err(wrong(
            kind,
            "length",
            if least == 0 {
                "a non-negative integer"
            } else {
                "a positive integer"
            },
            arg,
        )),
    }
}

/// A `prefix`: text, where an empty one is none.
fn prefix_of(kind: Kind, arg: &Arg) -> Result<Option<String>, TypeError> {
    match arg {
        Arg::Datum(Datum::Text(prefix)) => Ok((!prefix.is_empty()).then(|| prefix.clone())),
        _ => Err(wrong(kind, "prefix", "text", arg)),
    }
}

/// A `default`: `auto` when the kind makes a value up, or a plain value.
fn default_of(kind: Kind, arg: &Arg) -> Result<DefaultValue, TypeError> {
    match arg {
        Arg::Auto if kind.auto().is_some() => Ok(DefaultValue::Auto),
        Arg::Auto => Err(TypeError::AutoNotAllowed { ty: kind }),
        Arg::Datum(datum) if datum.is_finite() => Ok(DefaultValue::Given(datum.clone())),
        Arg::Datum(_) => Err(wrong(
            kind,
            "default",
            "a value whose numbers are finite",
            arg,
        )),
        Arg::Type(_) => Err(wrong(kind, "default", "a value or `auto`", arg)),
    }
}

/// An `overwrite`: a boolean (`true` or `false`).
fn overwrite_of(kind: Kind, arg: &Arg) -> Result<bool, TypeError> {
    match arg {
        Arg::Datum(Datum::Bool(b)) => Ok(*b),
        _ => Err(wrong(kind, "overwrite", "true or false", arg)),
    }
}
