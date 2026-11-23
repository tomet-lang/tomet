//! =[ tomet-eval ]
//!
//! Expression evaluation and document-level resolution for the `{$ ... }`
//! expressions of a Tomet document ([`InterpExpr`]), following
//! `tmtroot/spec/expressions/`.
//!
//! Uses `tomet-expr` (Layer 3) for the semantic model, types, and scopes,
//! `tomet-query` (Layer 6) for deterministic `#id` lookup and collection
//! filtering/sorting, and `tomet-transform` (Layer 6) for AST rewriting.

use std::collections::HashMap;

use tomet_ast::{
    BinaryOp, Block, File, InterpExpr, InterpExprKind, Placement, Tuple, TupleEntry, Value,
};
use tomet_declaration::{Arity, Placeholder, is_ident_placeholder, placeholder_of};
use tomet_expr::{
    CacheRequest, Closure, DirRef, DocRef, Env, EvalContext, EvalError, EvalValue, Namespace,
    RESERVED_VALUES, construct_with, datetime_member, evaluate_type_with, is_constructor,
    named_type,
};
use tomet_scalar::Scalar;
use tomet_span::Span;
use tomet_transform::{
    ApplyOutcome, Evaluator, Instantiated, SeatProblem, Source, Unresolved, apply_blueprint_with,
    instantiate_blueprint_with, resolve_expressions_where_with, resolve_expressions_with,
};
use tomet_tree::fill_interpolations;

mod address;
mod args;
mod chars;
mod fragments;
mod include;
mod methods;
mod ops;
mod parts;

#[cfg(test)]
mod resolve_tests;
#[cfg(test)]
mod tests;

pub use include::MAX_INCLUDE_DEPTH;

use args::Args;
use ops::{arithmetic, compare, values_equal};

/// The default expression evaluator backed by `tomet-eval`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Engine;

impl Evaluator for Engine {
    fn evaluate(&self, expr: &InterpExpr, cx: &EvalContext) -> Result<EvalValue, EvalError> {
        evaluate(expr, cx)
    }

    fn evaluate_data(&self, expr: &InterpExpr, cx: &EvalContext) -> Result<Value, EvalError> {
        evaluate_data(expr, cx)
    }
}

/// Replaces every `{$ ... }` in `doc` with what it stands for.
pub fn resolve_expressions(doc: &mut File, cx: &EvalContext) -> Vec<Unresolved> {
    resolve_expressions_with(doc, cx, &Engine)
}

/// [`resolve_expressions`] for the `{$ ... }` that `selected` picks.
pub fn resolve_expressions_where(
    doc: &mut File,
    cx: &EvalContext,
    selected: &dyn Fn(Span) -> bool,
) -> Vec<Unresolved> {
    resolve_expressions_where_with(doc, cx, selected, &Engine)
}

/// Instantiates a blueprint `doc`, filling its type seats from `source` and resolving expressions.
pub fn instantiate_blueprint(
    doc: &mut File,
    cx: &EvalContext,
    source: &dyn Source,
) -> Result<Instantiated, Vec<SeatProblem>> {
    instantiate_blueprint_with(doc, cx, source, &Engine)
}

/// Applies `blueprint` to an existing `doc`.
pub fn apply_blueprint(
    doc: &mut File,
    blueprint: &File,
    cx: &EvalContext,
    source: &dyn Source,
) -> Result<ApplyOutcome, Vec<SeatProblem>> {
    apply_blueprint_with(doc, blueprint, cx, source, &Engine)
}

/// Evaluates what a type seat (`<$list(string, length: 6)>`) holds using the full evaluator.
pub fn evaluate_type(expr: &InterpExpr, cx: &EvalContext) -> Result<tomet_type::Type, EvalError> {
    evaluate_type_with(expr, cx, &mut eval)
}

/// Evaluates `expr` in `cx`.
pub fn evaluate(expr: &InterpExpr, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    eval(expr, cx, &Env::new())
}

/// Whether `value` is, or holds, a document, a directory or an element: something that
/// is found through a vault and has no data form of its own.
fn refers_to_vault(value: &EvalValue) -> bool {
    match value {
        EvalValue::Document(_) | EvalValue::Directory(_) | EvalValue::Part { .. } => true,
        EvalValue::Tuple(tuple) => tuple.values().any(refers_to_vault),
        EvalValue::Call(_, args) => args.values().any(refers_to_vault),
        _ => false,
    }
}

/// Evaluates `expr` in `cx`, requiring the result to have a data representation.
pub fn evaluate_data(expr: &InterpExpr, cx: &EvalContext) -> Result<Value, EvalError> {
    let value = evaluate(expr, cx)?;
    value.to_ast().ok_or_else(|| EvalError::NotData {
        type_name: value.type_name(),
        is_vault_ref: refers_to_vault(&value),
    })
}

/// Calls a function value.
pub(crate) fn apply(
    f: &Closure,
    args: Vec<EvalValue>,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    if args.len() != f.params.len() {
        return Err(EvalError::WrongArgCount {
            function: "a function".to_string(),
            expected: f.params.len().to_string(),
            got: args.len(),
        });
    }
    let mut env = f.env.clone();
    for (param, arg) in f.params.iter().zip(args) {
        env = env.bind(param.clone(), arg);
    }
    eval(&f.body, cx, &env)
}

fn eval(expr: &InterpExpr, cx: &EvalContext, env: &Env) -> Result<EvalValue, EvalError> {
    match &expr.kind {
        InterpExprKind::Literal(literal) => Ok(EvalValue::from(literal.clone())),
        InterpExprKind::Group(inner) => eval(inner, cx, env),
        InterpExprKind::Seq(items) => {
            let mut entries = Vec::with_capacity(items.len());
            for item in items {
                match &item.kind {
                    InterpExprKind::NamedArg { name, value } => {
                        entries.push(TupleEntry::named(name.clone(), eval(value, cx, env)?));
                    }
                    _ => {
                        entries.push(TupleEntry::unnamed(eval(item, cx, env)?));
                    }
                }
            }
            Tuple::try_from_entries(entries)
                .map(EvalValue::Tuple)
                .map_err(|e| EvalError::message(format!("duplicate key `{}` in tuple", e.key)))
        }
        InterpExprKind::Identifier(name) => name_value(name, cx, env),
        InterpExprKind::Lambda { params, body } => {
            if let Some(reserved) = params
                .iter()
                .find(|p| RESERVED_VALUES.contains(&p.as_str()))
            {
                return Err(EvalError::message(format!(
                    "`{reserved}` is a reserved name and cannot be a function parameter"
                )));
            }
            Ok(EvalValue::Function(Closure {
                params: params.clone(),
                body: std::rc::Rc::new((**body).clone()),
                env: env.clone(),
            }))
        }
        InterpExprKind::Binary { op, lhs, rhs } => binary(*op, lhs, rhs, cx, env),
        InterpExprKind::Member { object, member } => {
            if matches!(&object.kind, InterpExprKind::Identifier(name) if name == "var")
                && env.get("var").is_none()
                && cx.var_unset.iter().any(|unset| unset == member)
            {
                return Err(EvalError::NoValue(member.clone()));
            }
            let value = eval(object, cx, env)?;
            member_value(value, member)
        }
        InterpExprKind::Connect { object, member } => {
            let value = eval(object, cx, env)?;
            connect_value(value, member, cx)
        }
        InterpExprKind::Call { callee, args } => call(callee, args, cx, env),
        InterpExprKind::NamedArg { name, .. } => Err(EvalError::message(format!(
            "`{name}: ...` is only meaningful as an argument of a call"
        ))),
        InterpExprKind::Id(id) => parts::part_by_id(&root_doc(cx, env), id, cx),
        InterpExprKind::DocId { doc, id } => {
            let doc = document_of("`#`", eval(doc, cx, env)?)?;
            parts::part_by_id(&doc, id, cx)
        }
        InterpExprKind::Addr { text, .. } => address::address_value(text, cx),
    }
}

fn name_value(name: &str, cx: &EvalContext, env: &Env) -> Result<EvalValue, EvalError> {
    if let Some(bound) = env.get(name) {
        return Ok(bound.clone());
    }
    match name {
        "self" => Ok(EvalValue::Document(cx.self_doc.clone())),
        "macro" => Ok(EvalValue::Namespace(Namespace::Macro)),
        "cache" => Ok(EvalValue::Namespace(Namespace::Cache)),
        "vault" => Ok(EvalValue::Directory(DirRef(String::new()))),
        "var" => Ok(cx.var.clone()),
        _ => named_type(name)
            .map(EvalValue::Type)
            .ok_or_else(|| EvalError::UnknownName(name.to_string())),
    }
}

fn connect_value(value: EvalValue, member: &str, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    match &value {
        EvalValue::Document(doc) => parts::document_connect(doc, member, cx),
        EvalValue::Part { doc, part } => parts::part_connect(doc, part, member),
        EvalValue::Directory(dir) => parts::directory_connect(dir, member, cx),
        other => Err(EvalError::message(format!(
            "`::{member}` goes from a node to what hangs off it, and a {} is not a node; \
             a key of its data is read with `.{member}`",
            other.type_name()
        ))),
    }
}

fn member_value(value: EvalValue, member: &str) -> Result<EvalValue, EvalError> {
    match &value {
        EvalValue::Document(doc) => parts::document_member(doc, member),
        EvalValue::Part { doc, part } => parts::part_member(doc, part, member),
        EvalValue::Directory(dir) => parts::directory_member(dir, member),
        EvalValue::DateTime(value) => datetime_member(value, member),
        EvalValue::Tuple(tuple) => {
            tuple
                .get(member)
                .cloned()
                .ok_or_else(|| EvalError::UnknownMember {
                    on: "tuple",
                    member: member.to_string(),
                })
        }
        EvalValue::Scalar(s) if !matches!(s, Scalar::None | Scalar::Auto) => {
            if member == "0" {
                Ok(value)
            } else {
                Err(EvalError::UnknownMember {
                    on: value.type_name(),
                    member: member.to_string(),
                })
            }
        }
        EvalValue::Namespace(Namespace::Macro) => Err(EvalError::message(format!(
            "the macro `{member}` has to be called: `macro.{member}(...)`"
        ))),
        EvalValue::Namespace(Namespace::Cache) => Err(EvalError::message(format!(
            "the cache kind `{member}` has to be called: `cache.{member}(\"...\")`"
        ))),
        _ => Err(EvalError::UnknownMember {
            on: value.type_name(),
            member: member.to_string(),
        }),
    }
}

fn eval_args(args: &[InterpExpr], cx: &EvalContext, env: &Env) -> Result<Args, EvalError> {
    let mut out = Args {
        positional: Vec::new(),
        named: Vec::new(),
    };
    for arg in args {
        match &arg.kind {
            InterpExprKind::NamedArg { name, value } => {
                out.named.push((name.clone(), eval(value, cx, env)?));
            }
            _ => out.positional.push(eval(arg, cx, env)?),
        }
    }
    Ok(out)
}

fn call(
    callee: &InterpExpr,
    args: &[InterpExpr],
    cx: &EvalContext,
    env: &Env,
) -> Result<EvalValue, EvalError> {
    match &callee.kind {
        InterpExprKind::Identifier(name) if is_constructor(name) => {
            construct_with(name, args, cx, env, &mut eval).map(EvalValue::Type)
        }
        InterpExprKind::Identifier(name) => {
            let args = eval_args(args, cx, env)?;
            builtin(name, args, cx)
        }
        InterpExprKind::Id(name) => {
            let args = eval_args(args, cx, env)?;
            root_item_call(name, &root_doc(cx, env), args, cx)
        }
        InterpExprKind::DocId { doc, id } => {
            let doc = document_of("`#`", eval(doc, cx, env)?)?;
            let args = eval_args(args, cx, env)?;
            root_item_call(id, &doc, args, cx)
        }
        InterpExprKind::Member { object, member } => {
            let receiver = eval(object, cx, env)?;
            if matches!(receiver, EvalValue::Namespace(Namespace::Cache)) {
                return cache_call(member, args, cx, env);
            }
            let args = eval_args(args, cx, env)?;
            match receiver {
                EvalValue::Namespace(Namespace::Macro) => macro_call(member, args, cx),
                EvalValue::Namespace(Namespace::Cache) => unreachable!("handled above"),
                EvalValue::Part { doc, part } => parts::part_method(&doc, &part, member, args),
                EvalValue::Document(doc) => parts::document_method(&doc, member, args, cx),
                EvalValue::Scalar(Scalar::Text(text)) if member == "addr" => {
                    args.exactly(".addr", 0)?;
                    address::address_value(&text, cx)
                }
                other => methods::call_method(other, member, args, cx),
            }
        }
        InterpExprKind::Connect { member, .. } => Err(EvalError::message(format!(
            "`::{member}` is not a method; a method is called with a dot, `.{member}(...)`"
        ))),
        _ => Err(EvalError::message("only a name or a member can be called")),
    }
}

fn cache_call(
    kind: &str,
    args: &[InterpExpr],
    cx: &EvalContext,
    env: &Env,
) -> Result<EvalValue, EvalError> {
    let mut positional = Vec::new();
    let mut local: Option<bool> = None;
    let mut inputs: Option<Vec<String>> = None;

    for arg in args {
        match &arg.kind {
            InterpExprKind::NamedArg { name, value } => {
                let evaluated = eval(value, cx, env)?;
                match name.as_str() {
                    "local" => {
                        if local.is_some() {
                            return Err(EvalError::message(format!(
                                "duplicate named argument `local` in `cache.{kind}`"
                            )));
                        }
                        match evaluated {
                            EvalValue::Scalar(Scalar::Bool(b)) => local = Some(b),
                            other => {
                                return Err(EvalError::mismatch(
                                    format!("`local` in `cache.{kind}`"),
                                    "a bool",
                                    other.type_name(),
                                ));
                            }
                        }
                    }
                    "inputs" => {
                        if inputs.is_some() {
                            return Err(EvalError::message(format!(
                                "duplicate named argument `inputs` in `cache.{kind}`"
                            )));
                        }
                        inputs = Some(parse_cache_inputs(kind, value, evaluated)?);
                    }
                    other => {
                        return Err(EvalError::message(format!(
                            "unknown named argument `{other}` in `cache.{kind}`"
                        )));
                    }
                }
            }
            _ => positional.push(eval(arg, cx, env)?),
        }
    }

    if positional.len() != 1 {
        return Err(EvalError::WrongArgCount {
            function: format!("cache.{kind}"),
            expected: "1".to_string(),
            got: positional.len(),
        });
    }
    let key = match positional.pop().expect("checked length 1") {
        EvalValue::Scalar(Scalar::Text(key)) => key,
        other => {
            return Err(EvalError::mismatch(
                format!("`cache.{kind}`"),
                "a string key",
                other.type_name(),
            ));
        }
    };

    cx.resolver.cache(&CacheRequest {
        kind: kind.to_string(),
        key,
        local: local.unwrap_or(false),
        inputs: inputs.unwrap_or_default(),
    })
}

fn parse_cache_inputs(
    kind: &str,
    expr: &InterpExpr,
    value: EvalValue,
) -> Result<Vec<String>, EvalError> {
    match value {
        EvalValue::Tuple(tuple) => {
            let mut out = Vec::with_capacity(tuple.len());
            for entry in tuple.entries() {
                if let Some(name) = entry.name() {
                    return Err(EvalError::message(format!(
                        "`inputs` in `cache.{kind}` takes unnamed strings, not named entry `{name}`"
                    )));
                }
                match entry.value() {
                    EvalValue::Scalar(Scalar::Text(s)) => out.push(s.clone()),
                    other => {
                        return Err(EvalError::mismatch(
                            format!("an entry of `inputs` in `cache.{kind}`"),
                            "a string",
                            other.type_name(),
                        ));
                    }
                }
            }
            Ok(out)
        }
        // Single-item `("scripts/deploy.sh")` parses as `Group(...)` in expressions.
        EvalValue::Scalar(Scalar::Text(s))
            if matches!(&expr.kind, InterpExprKind::Group(_)) =>
        {
            Ok(vec![s])
        }
        other => Err(EvalError::mismatch(
            format!("`inputs` in `cache.{kind}`"),
            "a tuple of strings",
            other.type_name(),
        )),
    }
}

fn macro_call(name: &str, args: Args, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let template = cx
        .macros
        .get(name)
        .ok_or_else(|| EvalError::UnknownFunction(format!("macro.{name}")))?;

    match template {
        Value::Scalar(Scalar::Text(template)) => {
            macro_args_fit(name, &Arity::of_text(template), &args)?;
            let mut text = template.clone();
            for (key, value) in &args.named {
                if is_ident_placeholder(key) {
                    text = text.replace(&format!("$({key})"), &methods::scalar_text(value)?);
                }
            }
            for (i, value) in args.positional.iter().enumerate() {
                text = text.replace(&format!("$({})", i + 1), &methods::scalar_text(value)?);
            }
            Ok(EvalValue::Scalar(Scalar::Text(text)))
        }
        Value::Blocks(_) | Value::Element(_) => {
            macro_args_fit(name, &Arity::of_body(template), &args)?;
            let fills = macro_fills(name, &args)?;
            let mut body = template.clone();
            fill_interpolations(&mut body, |interp| {
                placeholder_of(interp).and_then(|place| fills.get(&place).cloned())
            });
            match body {
                Value::Blocks(blocks) => Ok(EvalValue::Fragment(blocks)),
                Value::Element(mut el) => {
                    el.placement = Placement::Block;
                    Ok(EvalValue::Fragment(vec![Block::Element(*el)]))
                }
                other => unreachable!("filling a block or element macro left {other:?}"),
            }
        }
        _ => Err(EvalError::message(format!(
            "macro.{name} is not a string, block content, or element, so it cannot be called"
        ))),
    }
}

fn macro_fills(name: &str, args: &Args) -> Result<HashMap<Placeholder, Value>, EvalError> {
    let mut fills = HashMap::new();
    let named = args
        .named
        .iter()
        .filter(|(key, _)| is_ident_placeholder(key))
        .map(|(key, value)| (Placeholder::Named(key.clone()), value));
    let positional = args
        .positional
        .iter()
        .enumerate()
        .map(|(i, value)| (Placeholder::Positional(i + 1), value));
    for (place, value) in named.chain(positional) {
        let filled = value.to_ast().ok_or_else(|| {
            EvalError::mismatch(
                format!("an argument of macro.{name}"),
                "a value that can be written in a document",
                value.type_name(),
            )
        })?;
        fills.insert(place, filled);
    }
    Ok(fills)
}

fn macro_args_fit(name: &str, arity: &Arity, args: &Args) -> Result<(), EvalError> {
    if arity.positional > 0 && args.positional.len() != arity.positional {
        return Err(EvalError::WrongArgCount {
            function: format!("macro.{name}"),
            expected: arity.positional.to_string(),
            got: args.positional.len(),
        });
    }
    if arity.is_empty() && (!args.positional.is_empty() || !args.named.is_empty()) {
        return Err(EvalError::WrongArgCount {
            function: format!("macro.{name}"),
            expected: "0".to_string(),
            got: args.positional.len() + args.named.len(),
        });
    }
    for expected_key in &arity.named {
        if !args.named.iter().any(|(k, _)| k == expected_key) {
            return Err(EvalError::message(format!(
                "macro.{name} expects named argument `{expected_key}`"
            )));
        }
    }
    Ok(())
}

fn builtin(name: &str, args: Args, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    match name {
        "tmt" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::Scalar(Scalar::Text(path))) => {
                Ok(EvalValue::Document(DocRef(parts::normalize_path(&path))))
            }
            Some(other) => Err(EvalError::mismatch(
                "`tmt`",
                "a path string",
                other.type_name(),
            )),
            None => unreachable!("`exactly` returned one value"),
        },
        "ref" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::Scalar(Scalar::Text(note))) => cx
                .resolver
                .note(&note)
                .map(EvalValue::Document)
                .map_err(|why| EvalError::Unresolved(format!("note `{note}`: {why}"))),
            Some(other) => Err(EvalError::mismatch(
                "`ref`",
                "a note name",
                other.type_name(),
            )),
            None => unreachable!("`exactly` returned one value"),
        },
        "add" | "sub" | "mul" | "div" | "mod" => {
            let [a, b]: [EvalValue; 2] = args
                .exactly(name, 2)?
                .try_into()
                .unwrap_or_else(|_| unreachable!("`exactly` returned two values"));
            arithmetic(name, &a, &b)
        }
        "include" => include::include(args, cx),
        "link" => match args.between(name, 1, 2)?.as_slice() {
            [target] => fragments::link_fragment(target, None),
            [target, label] => fragments::link_fragment(target, Some(label)),
            _ => unreachable!("`between` returned 1 or 2 values"),
        },
        "raw" => fragments::raw_fragment(args),
        "emoji" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::Scalar(Scalar::Text(name))) => {
                let key = name.trim().trim_matches(':');
                chars::emoji_for(key)
                    .map(|emoji| EvalValue::Scalar(Scalar::Text(emoji.to_string())))
                    .ok_or_else(|| EvalError::message(format!("unknown emoji `{key}`")))
            }
            Some(other) => Err(EvalError::mismatch("`emoji`", "a name", other.type_name())),
            None => unreachable!("`exactly` returned one value"),
        },
        "unicode" => match args.exactly(name, 1)?.pop() {
            Some(code) => unicode_char(&code),
            None => unreachable!("`exactly` returned one value"),
        },
        "not" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::Scalar(Scalar::Bool(b))) => Ok(EvalValue::Scalar(Scalar::Bool(!b))),
            Some(other) => Err(EvalError::mismatch("`not`", "a bool", other.type_name())),
            None => unreachable!("`exactly` returned one value"),
        },
        "exists" => match args.exactly(name, 1)?.pop() {
            Some(value) => Ok(EvalValue::Scalar(Scalar::Bool(
                value != EvalValue::Scalar(Scalar::None),
            ))),
            None => unreachable!("`exactly` returned one value"),
        },
        "version" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::Scalar(Scalar::Text(text))) => {
                let v = tomet_type::Version::parse(text.trim()).ok_or_else(|| {
                    EvalError::message(format!("`{text}` is not a valid version"))
                })?;
                Ok(EvalValue::Version(v))
            }
            Some(EvalValue::Version(v)) => Ok(EvalValue::Version(v)),
            Some(EvalValue::Scalar(Scalar::Int(n))) if n >= 0 => {
                let v = tomet_type::Version::parse(&n.to_string())
                    .ok_or_else(|| EvalError::message(format!("`{n}` is not a valid version")))?;
                Ok(EvalValue::Version(v))
            }
            Some(EvalValue::Scalar(Scalar::Number(text))) => {
                let v = tomet_type::Version::parse(text.trim()).ok_or_else(|| {
                    EvalError::message(format!("`{text}` is not a valid version"))
                })?;
                Ok(EvalValue::Version(v))
            }
            Some(other) => Err(EvalError::mismatch(
                "`version`",
                "a version string or non-negative integer",
                other.type_name(),
            )),
            None => unreachable!("`exactly` returned one value"),
        },
        _ => Err(EvalError::UnknownFunction(name.to_string())),
    }
}

fn truth(op: &str, value: EvalValue) -> Result<bool, EvalError> {
    match value {
        EvalValue::Scalar(Scalar::Bool(b)) => Ok(b),
        other => Err(EvalError::mismatch(
            format!("`{op}`"),
            "a bool on both sides",
            other.type_name(),
        )),
    }
}

fn binary(
    op: BinaryOp,
    lhs: &InterpExpr,
    rhs: &InterpExpr,
    cx: &EvalContext,
    env: &Env,
) -> Result<EvalValue, EvalError> {
    match op {
        BinaryOp::And => {
            if !truth(BinaryOp::And.symbol(), eval(lhs, cx, env)?)? {
                return Ok(EvalValue::Scalar(Scalar::Bool(false)));
            }
            Ok(EvalValue::Scalar(Scalar::Bool(truth(
                BinaryOp::And.symbol(),
                eval(rhs, cx, env)?,
            )?)))
        }
        BinaryOp::Or => {
            if truth(BinaryOp::Or.symbol(), eval(lhs, cx, env)?)? {
                return Ok(EvalValue::Scalar(Scalar::Bool(true)));
            }
            Ok(EvalValue::Scalar(Scalar::Bool(truth(
                BinaryOp::Or.symbol(),
                eval(rhs, cx, env)?,
            )?)))
        }
        BinaryOp::Add | BinaryOp::Sub => {
            let (l, r) = (eval(lhs, cx, env)?, eval(rhs, cx, env)?);
            let function = if op == BinaryOp::Add { "add" } else { "sub" };
            arithmetic(function, &l, &r)
        }
        _ => {
            let (l, r) = (eval(lhs, cx, env)?, eval(rhs, cx, env)?);
            Ok(EvalValue::Scalar(Scalar::Bool(match op {
                BinaryOp::Eq => values_equal(&l, &r),
                BinaryOp::Ne => !values_equal(&l, &r),
                BinaryOp::Lt => compare(&l, &r)?.is_lt(),
                BinaryOp::Le => compare(&l, &r)?.is_le(),
                BinaryOp::Gt => compare(&l, &r)?.is_gt(),
                BinaryOp::Ge => compare(&l, &r)?.is_ge(),
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::And | BinaryOp::Or => {
                    unreachable!("handled above")
                }
            })))
        }
    }
}

fn root_doc(cx: &EvalContext, env: &Env) -> DocRef {
    match env.get(address::ROOT) {
        Some(EvalValue::Document(doc)) => doc.clone(),
        _ => cx.self_doc.clone(),
    }
}

fn document_of(what: &str, value: EvalValue) -> Result<DocRef, EvalError> {
    match value {
        EvalValue::Document(doc) => Ok(doc),
        other => Err(EvalError::mismatch(
            what,
            "a document on the left",
            other.type_name(),
        )),
    }
}

fn root_item_call(
    name: &str,
    doc: &DocRef,
    args: Args,
    cx: &EvalContext,
) -> Result<EvalValue, EvalError> {
    if name != "item" {
        return Err(EvalError::message(format!(
            "an element cannot be called: `#{name}(...)`"
        )));
    }
    match args.exactly("item", 1)?.pop() {
        Some(index) => parts::root_item(doc, &index, cx),
        None => unreachable!("`exactly` returned one value"),
    }
}

fn unicode_char(code: &EvalValue) -> Result<EvalValue, EvalError> {
    let bad = || EvalError::message(format!("`unicode` expects a code point, got {code:?}"));
    let point = match code {
        EvalValue::Scalar(Scalar::Int(i)) => u32::try_from(*i).map_err(|_| bad())?,
        EvalValue::Scalar(Scalar::Number(text)) => text.parse::<u32>().map_err(|_| bad())?,
        EvalValue::Scalar(Scalar::Text(text)) => {
            let digits = text
                .trim()
                .trim_start_matches("U+")
                .trim_start_matches("u+")
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            u32::from_str_radix(digits, 16).map_err(|_| bad())?
        }
        other => {
            return Err(EvalError::mismatch(
                "`unicode`",
                "a number or a hex string",
                other.type_name(),
            ));
        }
    };
    char::from_u32(point)
        .map(|c| EvalValue::Scalar(Scalar::Text(c.to_string())))
        .ok_or_else(bad)
}
