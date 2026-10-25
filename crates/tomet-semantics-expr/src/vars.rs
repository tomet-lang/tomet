//! The value of `var`: what the declared variables are, once a default or a
//! `--var` has been applied. See `tmtroot/spec/elements/std.vars.element.tmt`.
//!
//! The declarations come from `::vars` (a document's, over its vault's); what
//! was passed comes from `tomet new --var key=value`, and is text. A default
//! is read as written: a default that does not fit its `type` is left as it
//! is, and `tomet check` reports it, as for `typed_meta`. A value passed on
//! the command line is held to its type, because nothing else will.

use tomet_ast::Value;
use tomet_semantics::VarDecl;

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
        ty: String,
        text: String,
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
            (Some((_, Value::String(text))), _) => Some(from_text(name, decl, text)?),
            (Some((_, other)), _) => Some(EvalValue::from_ast(other, doc)),
            (None, Some(default)) => Some(typed_default(decl, EvalValue::from_ast(default, doc))),
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
fn from_text(name: &str, decl: &VarDecl, text: &str) -> Result<EvalValue, VarError> {
    let ty = decl.type_name();
    let bad = |why: &str| VarError::BadValue {
        name: name.to_string(),
        ty: ty.to_string(),
        text: text.to_string(),
        why: why.to_string(),
    };
    match ty {
        "string" => Ok(EvalValue::String(text.to_string())),
        "int" => text
            .trim()
            .parse::<i64>()
            .map(EvalValue::Int)
            .map_err(|_| bad("it is not an integer")),
        "float" => text
            .trim()
            .parse::<f64>()
            .map(EvalValue::Float)
            .map_err(|_| bad("it is not a number")),
        "bool" => match text.trim() {
            "true" => Ok(EvalValue::Bool(true)),
            "false" => Ok(EvalValue::Bool(false)),
            _ => Err(bad("it is neither `true` nor `false`")),
        },
        "datetime" => parse_datetime(text.trim())
            .map(EvalValue::DateTime)
            .ok_or_else(|| bad("it is not a datetime (RFC 3339)")),
        "list" => Err(bad("a list cannot be passed with `--var` yet")),
        _ => Err(bad("that is not a type a variable can have")),
    }
}

/// A default, shaped by the declared type where it plainly can be. One that
/// does not fit stays as written.
fn typed_default(decl: &VarDecl, value: EvalValue) -> EvalValue {
    match (decl.type_name(), value) {
        ("datetime", EvalValue::String(text)) => match parse_datetime(&text) {
            Some(parsed) => EvalValue::DateTime(parsed),
            None => EvalValue::String(text),
        },
        ("float", EvalValue::Int(i)) => EvalValue::Float(i as f64),
        ("list", list @ EvalValue::Seq(_)) => list,
        ("list", single) => EvalValue::Seq(vec![single]),
        (_, value) => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(ty: Option<&str>, default: Option<Value>) -> VarDecl {
        VarDecl {
            ty: ty.map(str::to_string),
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
            ("n".to_string(), decl(Some("int"), None)),
            ("on".to_string(), decl(Some("bool"), None)),
            ("at".to_string(), decl(Some("datetime"), None)),
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
            decl(Some("int"), Some(Value::String("many".to_string()))),
        )];
        let scope = var_scope(&decls, &doc(), &[]).unwrap();
        assert_eq!(
            scope.values,
            EvalValue::Map(vec![("n".into(), EvalValue::String("many".into()))])
        );
    }
}
