//! The value of `var`: what the declared variables are, once a default or a
//! `--var` has been applied. See `tmtroot/spec/elements/std.vars.element.tmt`.
//!
//! The declarations come from `::vars` (a document's, over its vault's); what
//! was passed comes from `tomet new --var key=value`, and is text. A default
//! is read as written: a default that does not fit its `type` is left as it
//! is, and `tomet check` reports it, as for `typed_meta`. A value passed on
//! the command line is held to its type, because nothing else will -- by
//! [`tomet_type::Type::read_text`]; what a type is, and what text it takes, is
//! that crate's to say.

use tomet_ast::Value;
use tomet_semantics::VarDecl;
use tomet_type::Type;

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
    /// A name passed that no `::vars` declares.
    Undeclared { name: String, declared: Vec<String> },
    /// A text that is not a value of the declared type.
    BadValue {
        name: String,
        ty: Type,
        text: String,
        /// What is wrong, as [`tomet_type::TextError`] says it: `` `many` is not an integer ``.
        why: String,
    },
}

impl std::fmt::Display for VarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VarError::Undeclared { name, declared } if declared.is_empty() => write!(
                f,
                "`--var {name}`: nothing declares a variable `{name}`; declare it in `::vars`"
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
            (Some((_, Value::String(text))), _) => {
                Some(typed_value(decl.ty, &read_text(name, decl.ty, text)?, doc))
            }
            (Some((_, other)), _) => Some(EvalValue::from_ast(other, doc)),
            (None, Some(default)) => Some(typed_value(decl.ty, default, doc)),
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
fn read_text(name: &str, ty: Type, text: &str) -> Result<Value, VarError> {
    ty.read_text(text).map_err(|err| VarError::BadValue {
        name: name.to_string(),
        ty,
        text: text.to_string(),
        why: err.to_string(),
    })
}

/// A value as the declared type reads it.
///
/// A type that is text -- `string`, `version`, `uuid`, `nanoid` -- reads a bare
/// scalar as the text it was written as, so `1.10` for a `version` is the string
/// `"1.10"` and not the number `1.1`, and `2024` for a `string` is `"2024"`.
/// For any other type the value is read as it is, then [`shaped`].
pub fn typed_value(ty: Type, value: &Value, doc: &DocRef) -> EvalValue {
    match (ty, value.scalar_text()) {
        (Type::String | Type::Version | Type::Uuid | Type::Nanoid, Some(text)) => {
            EvalValue::String(text.into_owned())
        }
        _ => shaped(ty, EvalValue::from_ast(value, doc)),
    }
}

/// A value, shaped by the declared type where it plainly can be: a datetime
/// that is text becomes a datetime, a whole number given for a `float` is a
/// number, a lone value for a `list` is a list of one. One that does not fit
/// stays as written -- `tomet check` is what reports it.
///
/// This is the one place a declared type shapes a value that was written for
/// it: a variable's default or `--var`, and a `::meta` field alike.
pub fn shaped(ty: Type, value: EvalValue) -> EvalValue {
    match (ty, value) {
        (Type::Datetime, EvalValue::String(text)) => match parse_datetime(&text) {
            Some(parsed) => EvalValue::DateTime(parsed),
            None => EvalValue::String(text),
        },
        (Type::Float, EvalValue::Int(i)) => EvalValue::Float(i as f64),
        (Type::List, list @ EvalValue::Seq(_)) => list,
        (Type::List, single) => EvalValue::Seq(vec![single]),
        (_, value) => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(ty: Option<Type>, default: Option<Value>) -> VarDecl {
        VarDecl {
            ty: ty.unwrap_or(Type::String),
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
            ("n".to_string(), decl(Some(Type::Int), None)),
            ("on".to_string(), decl(Some(Type::Bool), None)),
            ("at".to_string(), decl(Some(Type::Datetime), None)),
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
            decl(Some(Type::Int), Some(Value::String("many".to_string()))),
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
                decl(Some(Type::Version), Some(Value::Number("1.10".to_string()))),
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
        let decls = vec![("rev".to_string(), decl(Some(Type::Version), None))];
        let ok = var_scope(&decls, &doc(), &passed("rev", "1.10.0")).unwrap();
        assert_eq!(
            ok.values,
            EvalValue::Map(vec![("rev".into(), EvalValue::String("1.10.0".into()))])
        );
        assert!(var_scope(&decls, &doc(), &passed("rev", "1.0-beta")).is_err());
    }
}
