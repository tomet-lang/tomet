//! The value of `var`: what the declared variables are, once a default or a
//! `--var` has been applied. See `tmtroot/spec/elements/std.vars.element.tmt`.
//!
//! The declarations come from `::var` (a document's, over its vault's); what
//! was passed comes from `tomet new --var key=value`, and is text. A default
//! is read as written: a default that does not fit its `type` is left as it
//! is, and `tomet check` reports it, as for `typed_meta`. A value passed on
//! the command line is held to its type, because nothing else will: it is read
//! as the type's kind reads text, and a type that gives text a form of its own
//! holds it to that ([`Type::holds_text`]). What a type is, and what form its
//! text takes, is `tomet-type`'s to say.

use tomet_ast::Value;
use tomet_declaration::VarDecl;
use tomet_type::{Kind, Type, Version};

use crate::datetime::parse_datetime;
use crate::value::{DocRef, EvalValue};

/// The variables in scope: the map `var` stands for, and the declared names
/// that have no value to put in it.
#[derive(Debug, Clone, PartialEq)]
pub struct VarScope {
    /// A [`EvalValue::Map`] from name to value, in the order declared.
    pub values: EvalValue,
    /// Declared, no `default`, and none passed.
    pub unset: Vec<String>,
}

impl VarScope {
    /// No variables at all.
    pub fn empty() -> VarScope {
        VarScope {
            values: EvalValue::Map(Vec::new()),
            unset: Vec::new(),
        }
    }
}

/// Why `--var` could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarError {
    /// A name passed that no `::var` declares.
    Undeclared { name: String, declared: Vec<String> },
    /// A text that is not a value of the declared type.
    BadValue {
        name: String,
        /// Boxed: a type carries its arguments, and an error is not worth that much stack.
        ty: Box<Type>,
        text: String,
        /// What is wrong, as a sentence: `` `many` is not an integer ``.
        why: String,
    },
}

impl std::fmt::Display for VarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VarError::Undeclared { name, declared } if declared.is_empty() => write!(
                f,
                "`--var {name}`: nothing declares a variable `{name}`; declare it in `::var`"
            ),
            VarError::Undeclared { name, declared } => write!(
                f,
                "`--var {name}`: no variable `{name}` is declared; declared: {}",
                declared.join(", ")
            ),
            VarError::BadValue {
                name,
                ty,
                text,
                why,
            } => write!(
                f,
                "`--var {name}={text}`: `{name}` is declared `type: {ty}`, but {why}"
            ),
        }
    }
}

impl std::error::Error for VarError {}

/// Builds `var` from the declarations and the values passed.
///
/// `passed` is checked against the declarations: a name nothing declares, or a
/// text that is not a value of its type, is an error. A declared variable with
/// neither a passed value nor a `default` is not an error here -- it is listed
/// in [`VarScope::unset`], and whoever is creating a document refuses it.
pub fn var_scope(
    decls: &[(String, VarDecl)],
    doc: &DocRef,
    passed: &[(String, Value)],
) -> Result<VarScope, VarError> {
    for (name, _) in passed {
        if !decls.iter().any(|(declared, _)| declared == name) {
            return Err(VarError::Undeclared {
                name: name.clone(),
                declared: decls.iter().map(|(n, _)| n.clone()).collect(),
            });
        }
    }
    let mut values = Vec::new();
    let mut unset = Vec::new();
    for (name, decl) in decls {
        let given = passed.iter().rev().find(|(n, _)| n == name);
        let value = match (given, &decl.default) {
            (Some((_, Value::String(text))), _) => Some(typed_value(
                &decl.ty,
                &read_text(name, &decl.ty, text)?,
                doc,
            )),
            (Some((_, other)), _) => Some(EvalValue::from_ast(other, doc)),
            (None, Some(default)) => Some(typed_value(&decl.ty, default, doc)),
            (None, None) => None,
        };
        match value {
            Some(value) => values.push((name.clone(), value)),
            None => unset.push(name.clone()),
        }
    }
    Ok(VarScope {
        values: EvalValue::Map(values),
        unset,
    })
}

/// A text passed on the command line, as the value of the declared type.
fn read_text(name: &str, ty: &Type, text: &str) -> Result<Value, VarError> {
    let bad = |why: String| VarError::BadValue {
        name: name.to_string(),
        ty: Box::new(ty.clone()),
        text: text.to_string(),
        why,
    };
    match ty.kind() {
        Kind::String | Kind::Nanoid => Ok(Value::String(text.to_string())),
        Kind::Int => text
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| bad(format!("`{text}` is not an integer"))),
        Kind::Uint => match text.trim().parse::<i64>() {
            Ok(n) if n >= 0 => Ok(Value::Int(n)),
            _ => Err(bad(format!("`{text}` is not a non-negative integer"))),
        },
        Kind::Float => match text.trim().parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(Value::Float(n)),
            _ => Err(bad(format!("`{text}` is not a number"))),
        },
        Kind::Bool => match text.trim() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(bad(format!("`{text}` is not `true` or `false`"))),
        },
        Kind::Datetime | Kind::Date | Kind::Version | Kind::Uuid => {
            let trimmed = text.trim();
            if ty.holds_text(trimmed) == Some(true) {
                Ok(Value::String(trimmed.to_string()))
            } else {
                Err(bad(format!("`{text}` is not {}", ty.describe())))
            }
        }
        Kind::List => Err(bad("a list has no single-text form".to_string())),
    }
}

/// A value as the declared type reads it.
///
/// A type that is text -- `string`, `version`, `uuid`, `nanoid` -- reads a bare
/// scalar as the text it was written as, so `1.10` for a `version` is the string
/// `"1.10"` and not the number `1.1`, and `2024` for a `string` is `"2024"`.
/// For any other type the value is read as it is, then [`shaped`].
pub fn typed_value(ty: &Type, value: &Value, doc: &DocRef) -> EvalValue {
    match (ty.kind(), value.scalar_text()) {
        (Kind::String | Kind::Version | Kind::Uuid | Kind::Nanoid, Some(text)) => {
            EvalValue::String(text.into_owned())
        }
        _ => shaped(ty, EvalValue::from_ast(value, doc)),
    }
}

/// A value, shaped by the declared type where it plainly can be: a datetime or a
/// date that is text becomes a datetime (a date is midnight, UTC), a whole number
/// given for a `float` is a number, a lone value for a `list` is a list of one.
/// One that does not fit stays as written -- `tomet check` is what reports it.
///
/// This is the one place a declared type shapes a value that was written for
/// it: a variable's default or `--var`, and a `::meta` field alike.
pub fn shaped(ty: &Type, value: EvalValue) -> EvalValue {
    match (ty.kind(), value) {
        (Kind::Datetime, EvalValue::String(text)) => match parse_datetime(&text) {
            Some(parsed) => EvalValue::DateTime(parsed),
            None => EvalValue::String(text),
        },
        (Kind::Date, EvalValue::String(text)) => {
            match parse_datetime(&text).filter(|_| ty.holds_text(&text) == Some(true)) {
                Some(parsed) => EvalValue::DateTime(parsed),
                None => EvalValue::String(text),
            }
        }
        (Kind::Version, EvalValue::String(text)) => match Version::parse(text.trim()) {
            Some(v) => EvalValue::Version(v),
            None => EvalValue::String(text),
        },
        (Kind::Version, EvalValue::Int(n)) if n >= 0 => match Version::parse(&n.to_string()) {
            Some(v) => EvalValue::Version(v),
            None => EvalValue::Int(n),
        },
        (Kind::Version, EvalValue::Number(text)) => match Version::parse(text.trim()) {
            Some(v) => EvalValue::Version(v),
            None => EvalValue::Number(text),
        },
        (Kind::Float, EvalValue::Int(i)) => EvalValue::Float(i as f64),
        (Kind::Float, EvalValue::Number(text)) => match text.parse::<f64>() {
            Ok(f) => EvalValue::Float(f),
            Err(_) => EvalValue::Number(text),
        },
        (Kind::Int, EvalValue::Number(text)) => match text.parse::<i64>() {
            Ok(i) => EvalValue::Int(i),
            Err(_) => EvalValue::Number(text),
        },
        (Kind::Uint, EvalValue::Number(text)) => match text.parse::<i64>() {
            Ok(n) if n >= 0 => EvalValue::Int(n),
            _ => EvalValue::Number(text),
        },
        (Kind::List, EvalValue::Seq(items)) => match ty.item() {
            Some(item) => {
                EvalValue::Seq(items.into_iter().map(|value| shaped(item, value)).collect())
            }
            None => EvalValue::Seq(items),
        },
        (Kind::List, single) => EvalValue::Seq(vec![match ty.item() {
            Some(item) => shaped(item, single),
            None => single,
        }]),
        (_, value) => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bare(kind: Kind) -> Type {
        Type::bare(kind)
    }

    fn decl(ty: Option<Type>, default: Option<Value>) -> VarDecl {
        VarDecl {
            ty: ty.unwrap_or(bare(Kind::String)),
            default,
            about: None,
        }
    }

    fn doc() -> DocRef {
        DocRef("a.tmt".to_string())
    }

    fn passed(name: &str, text: &str) -> Vec<(String, Value)> {
        vec![(name.to_string(), Value::String(text.to_string()))]
    }

    #[test]
    fn a_default_is_the_value_and_a_passed_text_wins() {
        let decls = vec![(
            "who".to_string(),
            decl(None, Some(Value::String("me".to_string()))),
        )];
        let scope = var_scope(&decls, &doc(), &[]).unwrap();
        assert_eq!(
            scope.values,
            EvalValue::Map(vec![("who".into(), EvalValue::String("me".into()))])
        );
        let scope = var_scope(&decls, &doc(), &passed("who", "you")).unwrap();
        assert_eq!(
            scope.values,
            EvalValue::Map(vec![("who".into(), EvalValue::String("you".into()))])
        );
    }

    #[test]
    fn a_variable_with_no_default_and_none_passed_is_unset() {
        let decls = vec![("title".to_string(), decl(None, None))];
        let scope = var_scope(&decls, &doc(), &[]).unwrap();
        assert_eq!(scope.unset, ["title"]);
        assert_eq!(scope.values, EvalValue::Map(Vec::new()));
    }

    #[test]
    fn a_name_nothing_declares_is_refused_and_the_declared_ones_are_listed() {
        let decls = vec![("title".to_string(), decl(None, None))];
        let err = var_scope(&decls, &doc(), &passed("titel", "x")).unwrap_err();
        assert_eq!(
            err,
            VarError::Undeclared {
                name: "titel".into(),
                declared: vec!["title".into()]
            }
        );
        assert!(err.to_string().contains("declared: title"), "{err}");
    }

    #[test]
    fn a_passed_text_is_held_to_the_declared_type() {
        let decls = vec![
            ("n".to_string(), decl(Some(bare(Kind::Int)), None)),
            ("on".to_string(), decl(Some(bare(Kind::Bool)), None)),
            ("at".to_string(), decl(Some(bare(Kind::Datetime)), None)),
        ];
        let scope = var_scope(&decls, &doc(), &passed("n", "3")).unwrap();
        assert_eq!(
            scope.values,
            EvalValue::Map(vec![("n".into(), EvalValue::Int(3))])
        );
        for (name, text) in [("n", "three"), ("on", "yes"), ("at", "tomorrow")] {
            let err = var_scope(&decls, &doc(), &passed(name, text)).unwrap_err();
            assert!(matches!(err, VarError::BadValue { .. }), "{name}: {err}");
        }
        assert!(var_scope(&decls, &doc(), &passed("at", "2026-10-25T09:00:00+09:00")).is_ok());
    }

    #[test]
    fn a_default_that_does_not_fit_its_type_is_left_as_written() {
        // `tomet check` reports it; rendering does not stop for it.
        let decls = vec![(
            "n".to_string(),
            decl(
                Some(bare(Kind::Int)),
                Some(Value::String("many".to_string())),
            ),
        )];
        let scope = var_scope(&decls, &doc(), &[]).unwrap();
        assert_eq!(
            scope.values,
            EvalValue::Map(vec![("n".into(), EvalValue::String("many".into()))])
        );
    }

    #[test]
    fn a_text_variable_is_the_text_it_was_written_as() {
        let decls = vec![
            (
                "rev".to_string(),
                decl(
                    Some(bare(Kind::Version)),
                    Some(Value::Number("1.10".to_string())),
                ),
            ),
            ("label".to_string(), decl(None, Some(Value::Int(2024)))),
        ];
        let scope = var_scope(&decls, &doc(), &[]).unwrap();
        assert_eq!(
            scope.values,
            EvalValue::Map(vec![
                ("rev".into(), EvalValue::String("1.10".into())),
                ("label".into(), EvalValue::String("2024".into())),
            ])
        );
        // passed as text, a version is held to its form
        let decls = vec![("rev".to_string(), decl(Some(bare(Kind::Version)), None))];
        let ok = var_scope(&decls, &doc(), &passed("rev", "1.10.0")).unwrap();
        assert_eq!(
            ok.values,
            EvalValue::Map(vec![("rev".into(), EvalValue::String("1.10.0".into()))])
        );
        let ok_beta = var_scope(&decls, &doc(), &passed("rev", "1.0-beta")).unwrap();
        assert_eq!(
            ok_beta.values,
            EvalValue::Map(vec![("rev".into(), EvalValue::String("1.0-beta".into()))])
        );
        assert!(var_scope(&decls, &doc(), &passed("rev", "1.0.0_beta")).is_err());
    }

    #[test]
    fn a_date_is_held_to_its_form_and_shaped_like_a_datetime() {
        let decls = vec![("day".to_string(), decl(Some(bare(Kind::Date)), None))];
        // A date is a calendar day and nothing after it.
        for text in ["2026-10-25T09:00:00Z", "2026-02-29", "tomorrow"] {
            let err = var_scope(&decls, &doc(), &passed("day", text)).unwrap_err();
            assert!(matches!(err, VarError::BadValue { .. }), "{text}: {err}");
        }
        let err = var_scope(&decls, &doc(), &passed("day", "2026-13-01")).unwrap_err();
        assert_eq!(
            err.to_string(),
            "`--var day=2026-13-01`: `day` is declared `type: date`, but `2026-13-01` is not a date such as `2026-10-25`"
        );
        let scope = var_scope(&decls, &doc(), &passed("day", "2026-10-25")).unwrap();
        let EvalValue::Map(entries) = scope.values else {
            panic!("a map");
        };
        assert_eq!(
            entries[0].1,
            EvalValue::DateTime(parse_datetime("2026-10-25").unwrap())
        );
        // What is not a date stays as written, for `tomet check` to report.
        assert_eq!(
            shaped(
                &bare(Kind::Date),
                EvalValue::String("2026-10-25T09:00".into())
            ),
            EvalValue::String("2026-10-25T09:00".into())
        );
    }
}
