//! Types in the expression language: a type name is a type, a call of one
//! builds a type, and a type seat's written type is read with the types first.
//!
//! What a type may be given is [`Type::build`]'s to say, and this module adds
//! nothing to the rules: it turns what was written -- evaluated arguments, and
//! `auto` for `default` -- into the arguments `build` takes, and `build`'s
//! refusals come out as [`EvalError::BadType`] in its own words.

use tomet_ast::{InterpExpr, InterpExprKind, Tuple, TupleEntry};
use tomet_semantics_core::value_to_datum;
use tomet_type::{Arg, Datum, Kind, Type, TypeError};

use super::context::EvalContext;
use super::error::EvalError;
use super::value::{DirRef, Env, EvalValue, Namespace};

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
pub fn named(name: &str) -> Option<Type> {
    Kind::from_name(name).map(Type::bare)
}

/// Whether a call of `name` builds a type: it is the name of a kind, and no
/// function has it (`version("1.10")` is a function, and stays one).
pub fn is_constructor(name: &str) -> bool {
    Kind::from_name(name).is_some() && !super::is_function(name)
}

/// Builds the type `name(args)` using `eval_arg` to evaluate argument expressions.
///
/// Arguments are evaluated: `list(string)` is given the type `string`, and `default: auto`
/// is given the value `auto`.
pub fn construct_with(
    name: &str,
    args: &[InterpExpr],
    cx: &EvalContext,
    env: &Env,
    eval_arg: &mut dyn FnMut(&InterpExpr, &EvalContext, &Env) -> Result<EvalValue, EvalError>,
) -> Result<Type, EvalError> {
    let mut positional: Vec<Arg> = Vec::new();
    let mut named: Vec<(&str, Arg)> = Vec::new();
    for arg in args {
        match &arg.kind {
            InterpExprKind::NamedArg { name: key, value } => {
                let mut arg = argument(name, eval_arg(value, cx, env)?)?;
                if name == "list"
                    && key == "default"
                    && matches!(&value.kind, InterpExprKind::Group(_))
                {
                    if let Arg::Datum(d) = arg {
                        arg = Arg::Datum(Datum::Seq(vec![d]));
                    }
                }
                named.push((key.as_str(), arg));
            }
            _ => positional.push(argument(name, eval_arg(arg, cx, env)?)?),
        }
    }
    Type::build(name, &positional, &named).map_err(EvalError::BadType)
}

fn eval_type_arg(expr: &InterpExpr, cx: &EvalContext, env: &Env) -> Result<EvalValue, EvalError> {
    match &expr.kind {
        InterpExprKind::Literal(literal) => Ok(EvalValue::from(literal.clone())),
        InterpExprKind::Group(inner) => eval_type_arg(inner, cx, env),
        InterpExprKind::Seq(items) => {
            let mut entries = Vec::with_capacity(items.len());
            for item in items {
                match &item.kind {
                    InterpExprKind::NamedArg { name, value } => {
                        entries.push(TupleEntry::named(
                            name.clone(),
                            eval_type_arg(value, cx, env)?,
                        ));
                    }
                    _ => {
                        entries.push(TupleEntry::unnamed(eval_type_arg(item, cx, env)?));
                    }
                }
            }
            Tuple::try_from_entries(entries)
                .map(EvalValue::Tuple)
                .map_err(|e| EvalError::message(format!("duplicate key `{}` in tuple", e.key)))
        }
        InterpExprKind::Identifier(name) => {
            if let Some(bound) = env.get(name) {
                return Ok(bound.clone());
            }
            match name.as_str() {
                "self" => Ok(EvalValue::Document(cx.self_doc.clone())),
                "macro" => Ok(EvalValue::Namespace(Namespace::Macro)),
                "cache" => Ok(EvalValue::Namespace(Namespace::Cache)),
                "vault" => Ok(EvalValue::Directory(DirRef(String::new()))),
                "var" => Ok(cx.var.clone()),
                _ => named(name)
                    .map(EvalValue::Type)
                    .ok_or_else(|| EvalError::UnknownName(name.to_string())),
            }
        }
        InterpExprKind::Call { callee, args } => match &callee.kind {
            InterpExprKind::Identifier(name) if is_constructor(name) => {
                construct_with(name, args, cx, env, &mut eval_type_arg).map(EvalValue::Type)
            }
            InterpExprKind::Identifier(name) if super::is_function(name) => {
                Err(EvalError::message(format!(
                    "a type seat holds a type, and `{expr}` is a call to `{name}`"
                )))
            }
            InterpExprKind::Identifier(name) => Err(EvalError::UnknownFunction(name.clone())),
            _ => Err(EvalError::message(
                "only a type constructor can be called here",
            )),
        },
        InterpExprKind::Member { object, member } => {
            if matches!(&object.kind, InterpExprKind::Identifier(name) if name == "var")
                && env.get("var").is_none()
                && cx.var_unset.iter().any(|unset| unset == member)
            {
                return Err(EvalError::NoValue(member.clone()));
            }
            match eval_type_arg(object, cx, env)? {
                EvalValue::Tuple(tuple) => {
                    tuple
                        .get(member)
                        .cloned()
                        .ok_or_else(|| EvalError::UnknownMember {
                            on: "tuple",
                            member: member.to_string(),
                        })
                }
                other => Err(EvalError::UnknownMember {
                    on: other.type_name(),
                    member: member.to_string(),
                }),
            }
        }
        _ => Err(EvalError::message(
            "unsupported expression in type argument",
        )),
    }
}

/// An evaluated value as an argument of the type `ty`: a type, or plain data.
fn argument(ty: &str, value: EvalValue) -> Result<Arg, EvalError> {
    if let EvalValue::Type(inner) = value {
        return Ok(Arg::Type(inner));
    }
    value
        .to_ast()
        .as_ref()
        .and_then(value_to_datum)
        .map(Arg::Datum)
        .ok_or_else(|| {
            EvalError::mismatch(
                format!("an argument of the type `{ty}`"),
                "a type or plain data: a number, text, `true`, `false`, `none`, `auto` or a list of them",
                value.type_name(),
            )
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
    evaluate_type_with(expr, cx, &mut eval_type_arg)
}

/// [`evaluate_type`] using a caller-supplied evaluator for non-type subexpressions.
pub fn evaluate_type_with(
    expr: &InterpExpr,
    cx: &EvalContext,
    eval_arg: &mut dyn FnMut(&InterpExpr, &EvalContext, &Env) -> Result<EvalValue, EvalError>,
) -> Result<Type, EvalError> {
    let env = Env::new();
    let written = match &expr.kind {
        InterpExprKind::Identifier(name) => named(name).map(Ok),
        InterpExprKind::Call { callee, args } => match &callee.kind {
            InterpExprKind::Identifier(name) if Kind::from_name(name).is_some() => {
                Some(construct_with(name, args, cx, &env, eval_arg))
            }
            _ => None,
        },
        _ => None,
    };
    let value = match written {
        Some(ty) => EvalValue::Type(ty?),
        None => eval_arg(expr, cx, &env).map_err(|error| unknown_type(expr, error))?,
    };
    match value {
        EvalValue::Type(ty) => Ok(ty),
        other => Err(EvalError::message(format!(
            "a type seat holds a type, and `{expr}` is a {}",
            other.type_name()
        ))),
    }
}
