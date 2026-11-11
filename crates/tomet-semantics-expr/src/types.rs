//! Types in the expression language: a type name is a type, a call of one
//! builds a type, and a type seat's written type is read with the types first.
//!
//! What a type may be given is [`Type::build`]'s to say, and this module adds
//! nothing to the rules: it turns what was written -- evaluated arguments, and
//! `auto` for `default` -- into the arguments `build` takes, and `build`'s
//! refusals come out as [`EvalError::BadType`] in its own words.

use tomet_ast::{InterpExpr, InterpExprKind, Value};
use tomet_type::{Arg, Datum, Kind, Type, TypeError};

use super::context::EvalContext;
use super::error::EvalError;
use super::value::{Env, EvalValue};

/// The names of the types, each with a line saying what it is: what an editor
/// offers where an expression may start.
///
/// One name for every [`Kind`], so a type added there is offered without the
/// editor being told.
pub fn type_names() -> Vec<(&'static str, String)> {
    Kind::ALL
        .into_iter()
        .map(|kind| {
            let what = Type::bare(kind).describe();
            (kind.name(), format!("The type of {what}"))
        })
        .collect()
}

/// The type a bare name stands for: `string`, `list`, `uuid`.
pub(crate) fn named(name: &str) -> Option<Type> {
    Kind::from_name(name).map(Type::bare)
}

/// Whether a call of `name` builds a type: it is the name of a kind, and no
/// function has it (`version("1.10")` is a function, and stays one).
pub(crate) fn is_constructor(name: &str) -> bool {
    Kind::from_name(name).is_some() && !super::is_function(name)
}

/// Builds the type `name(args)`.
///
/// Arguments by position are evaluated, so `list(string)` is given the type
/// `string`. `auto` is not a name: it is read here, and only as the value of
/// the argument `default`, which is why this takes the arguments as written
/// and not as evaluated.
pub(crate) fn construct(
    name: &str,
    args: &[InterpExpr],
    cx: &EvalContext,
    env: &Env,
) -> Result<Type, EvalError> {
    let mut positional: Vec<Arg> = Vec::new();
    let mut named: Vec<(&str, Arg)> = Vec::new();
    for arg in args {
        match &arg.kind {
            InterpExprKind::NamedArg { name: key, value } => {
                let arg = if key == "default" && is_auto(value) {
                    Arg::Auto
                } else {
                    let mut arg = argument(name, super::eval(value, cx, env)?)?;
                    if name == "list"
                        && key == "default"
                        && matches!(&value.kind, InterpExprKind::Group(_))
                    {
                        if let Arg::Datum(d) = arg {
                            arg = Arg::Datum(Datum::Seq(vec![d]));
                        }
                    }
                    arg
                };
                named.push((key.as_str(), arg));
            }
            _ => positional.push(argument(name, super::eval(arg, cx, env)?)?),
        }
    }
    Type::build(name, &positional, &named).map_err(EvalError::BadType)
}

/// Whether `value` is the bare word `auto`.
fn is_auto(value: &InterpExpr) -> bool {
    matches!(&value.kind, InterpExprKind::Identifier(word) if word == "auto")
}

/// An evaluated value as an argument of the type `ty`: a type, or plain data.
fn argument(ty: &str, value: EvalValue) -> Result<Arg, EvalError> {
    if let EvalValue::Type(inner) = value {
        return Ok(Arg::Type(inner));
    }
    value
        .to_ast()
        .as_ref()
        .and_then(datum)
        .map(Arg::Datum)
        .ok_or_else(|| {
            EvalError::mismatch(
                format!("an argument of the type `{ty}`"),
                "a type or plain data: a number, text, `true`, `false`, `null` or a list of them",
                value.type_name(),
            )
        })
}

/// The plain value a data value is, or `None` for one that is more than that
/// (a map, an element, content).
fn datum(value: &Value) -> Option<Datum> {
    Some(match value {
        Value::Null => Datum::Null,
        Value::Bool(b) => Datum::Bool(*b),
        Value::Int(n) => Datum::Int(*n),
        Value::Float(n) => Datum::Float(*n),
        Value::Number(text) => text
            .parse::<i64>()
            .ok()
            .map(Datum::Int)
            .or_else(|| text.parse::<f64>().ok().map(Datum::Float))?,
        Value::String(text) => Datum::Text(text.clone()),
        Value::Seq(items) => Datum::Seq(items.iter().map(datum).collect::<Option<_>>()?),
        _ => return None,
    })
}

/// What a seat holds is a type, so a head that is no name of one is an unknown type, in the type
/// crate's words, and not an unknown name or function of the expression language.
fn unknown_type(expr: &InterpExpr, error: EvalError) -> EvalError {
    let head = match (&expr.kind, &error) {
        (InterpExprKind::Identifier(name), EvalError::UnknownName(unknown)) if name == unknown => {
            name
        }
        (InterpExprKind::Call { callee, .. }, EvalError::UnknownFunction(unknown)) => {
            match &callee.kind {
                InterpExprKind::Identifier(name) if name == unknown => name,
                _ => return error,
            }
        }
        _ => return error,
    };
    EvalError::BadType(TypeError::Unknown(head.clone()))
}

/// Evaluates what a type seat (`<$list(string, length: 6)>`) holds: the type it
/// is written as.
///
/// The same language as any expression, with the types first: the head of the
/// seat is a type wherever a function has the same name, so `<$version>` and
/// `<$version(default: "1.0")>` are types and not the function `version(...)`.
/// Anything the seat holds that is not a type is an error, and so is a type that
/// is not well formed -- an argument it does not take, `default: auto` for a type
/// that makes no value up.
pub fn evaluate_type(expr: &InterpExpr, cx: &EvalContext) -> Result<Type, EvalError> {
    let env = Env::new();
    let written = match &expr.kind {
        InterpExprKind::Identifier(name) => named(name).map(Ok),
        InterpExprKind::Call { callee, args } => match &callee.kind {
            InterpExprKind::Identifier(name) if Kind::from_name(name).is_some() => {
                Some(construct(name, args, cx, &env))
            }
            _ => None,
        },
        _ => None,
    };
    let value = match written {
        Some(ty) => EvalValue::Type(ty?),
        None => super::eval(expr, cx, &env).map_err(|error| unknown_type(expr, error))?,
    };
    match value {
        EvalValue::Type(ty) => Ok(ty),
        other => Err(EvalError::message(format!(
            "a type seat holds a type, and `{expr}` is a {}",
            other.type_name()
        ))),
    }
}
