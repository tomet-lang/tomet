//! The expression evaluator.
//!
//! Built beside the older `evaluate*` functions in the crate root, which keep
//! serving their callers until those move over. This one follows
//! `tmtroot/spec/expressions/`: values are typed ([`EvalValue`]), a name is
//! looked up in the one table its position selects, an operator or a method
//! never converts a value it was not given, and what changes from call to
//! call is confined to document creation.

mod address;
mod args;
mod context;
mod datetime;
mod error;
mod fragments;
mod memory;
mod methods;
mod nodes;
mod ops;
mod value;

#[cfg(test)]
mod tests;

pub use context::{EvalContext, Generate, NoResolver, Phase, Resolver, SystemGenerate};
pub use datetime::{format_datetime, parse_datetime};
pub use error::EvalError;
pub use memory::MemoryResolver;
pub use value::{Closure, DirRef, DocRef, Env, EvalValue, Namespace, Node};

use tomet_ast::{BinaryOp, InterpExpr, InterpExprKind, Literal};

use args::Args;
use ops::{arithmetic, compare, values_equal};

/// The names that stand for a value and that no lambda parameter may take.
/// `vars` and `generate` exist only while a document is being created.
pub const RESERVED_VALUES: [&str; 5] = ["self", "macro", "vault", "vars", "generate"];

/// Evaluates `expr` in `cx`.
pub fn evaluate(expr: &InterpExpr, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    eval(expr, cx, &Env::new())
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
        InterpExprKind::Literal(literal) => Ok(match literal {
            Literal::Int(i) => EvalValue::Int(*i),
            Literal::Float(f) => EvalValue::Float(*f),
            Literal::String(s) => EvalValue::String(s.clone()),
            Literal::Bool(b) => EvalValue::Bool(*b),
            Literal::Null => EvalValue::Null,
        }),
        InterpExprKind::Group(inner) => eval(inner, cx, env),
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
            let value = eval(object, cx, env)?;
            member_value(value, member, cx)
        }
        InterpExprKind::Call { callee, args } => call(callee, args, cx, env),
        InterpExprKind::NamedArg { name, .. } => Err(EvalError::message(format!(
            "`{name}: ...` is only meaningful as an argument of a call"
        ))),
        InterpExprKind::Id(id) => nodes::node_by_id(&root_doc(cx, env), id, cx),
        InterpExprKind::DocId { doc, id } => {
            let doc = document_of("`#`", eval(doc, cx, env)?)?;
            nodes::node_by_id(&doc, id, cx)
        }
        InterpExprKind::Addr { text, .. } => address::address_value(text, cx),
    }
}

/// A name written as a value: a lambda parameter, else a reserved value.
fn name_value(name: &str, cx: &EvalContext, env: &Env) -> Result<EvalValue, EvalError> {
    if let Some(bound) = env.get(name) {
        return Ok(bound.clone());
    }
    match name {
        "self" => Ok(EvalValue::Document(cx.self_doc.clone())),
        "macro" => Ok(EvalValue::Namespace(Namespace::Macro)),
        "vault" => Ok(EvalValue::Directory(DirRef(String::new()))),
        "vars" | "generate" if cx.phase == Phase::Output => {
            Err(EvalError::PhaseOnly(name.to_string()))
        }
        "vars" => Ok(cx.vars.clone()),
        "generate" => Ok(EvalValue::Namespace(Namespace::Generate)),
        _ => Err(EvalError::UnknownName(name.to_string())),
    }
}

/// `value.member`, without a call.
fn member_value(value: EvalValue, member: &str, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    match &value {
        EvalValue::Document(doc) => nodes::document_member(doc, member, cx),
        EvalValue::Node { doc, node } => nodes::node_member(doc, node, member),
        EvalValue::Directory(dir) => nodes::directory_member(dir, member, cx),
        EvalValue::Map(entries) => entries
            .iter()
            .find(|(k, _)| k == member)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| EvalError::UnknownMember {
                on: "map",
                member: member.to_string(),
            }),
        EvalValue::Namespace(Namespace::Macro) => Err(EvalError::message(format!(
            "the macro `{member}` has to be called: `macro.{member}(...)`"
        ))),
        EvalValue::Namespace(Namespace::Generate) => Err(EvalError::message(format!(
            "`generate.{member}` has to be called: `generate.{member}()`"
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
            let args = eval_args(args, cx, env)?;
            match receiver {
                EvalValue::Namespace(Namespace::Macro) => macro_call(member, args, cx),
                EvalValue::Namespace(Namespace::Generate) => generate_call(member, args, cx),
                EvalValue::Node { doc, node } => nodes::node_method(&doc, &node, member, args),
                EvalValue::Document(doc) => nodes::document_method(&doc, member, args, cx),
                EvalValue::String(text) if member == "addr" => {
                    args.exactly(".addr", 0)?;
                    address::address_value(&text, cx)
                }
                other => methods::call_method(other, member, args, cx),
            }
        }
        _ => Err(EvalError::message("only a name or a member can be called")),
    }
}

/// `generate.now()` and `generate.uuid()`.
fn generate_call(name: &str, args: Args, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    if cx.phase == Phase::Output {
        return Err(EvalError::PhaseOnly(format!("generate.{name}")));
    }
    args.exactly(&format!("generate.{name}"), 0)?;
    match name {
        "now" => Ok(EvalValue::DateTime(cx.generate.now())),
        "uuid" => Ok(EvalValue::String(cx.generate.uuid())),
        _ => Err(EvalError::UnknownFunction(format!("generate.{name}"))),
    }
}

/// `macro.name(args)`: the template with its placeholders filled in.
/// `${1}`, `${2}` take the positional arguments and `${key}` the named ones.
fn macro_call(name: &str, args: Args, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let template = cx
        .macros
        .get(name)
        .ok_or_else(|| EvalError::UnknownFunction(format!("macro.{name}")))?;
    let mut text = template.clone();
    for (key, value) in &args.named {
        text = text.replace(&format!("${{{key}}}"), &methods::scalar_text(value)?);
    }
    for (i, value) in args.positional.iter().enumerate() {
        text = text.replace(&format!("${{{}}}", i + 1), &methods::scalar_text(value)?);
    }
    Ok(EvalValue::String(text))
}

/// The functions that are called by a bare name.
fn builtin(name: &str, args: Args, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    match name {
        "tmt" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::String(path)) => {
                Ok(EvalValue::Document(DocRef(nodes::normalize_path(&path))))
            }
            Some(other) => Err(EvalError::mismatch(
                "`tmt`",
                "a path string",
                other.type_name(),
            )),
            None => unreachable!("`exactly` returned one value"),
        },
        "ref" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::String(note)) => cx
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
        "link" => match args.exactly(name, 1)?.pop() {
            Some(target) => fragments::link_fragment(&target),
            None => unreachable!("`exactly` returned one value"),
        },
        "emoji" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::String(name)) => {
                let key = name.trim().trim_matches(':');
                crate::functions::emoji_for(key)
                    .map(|emoji| EvalValue::String(emoji.to_string()))
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
            Some(EvalValue::Bool(b)) => Ok(EvalValue::Bool(!b)),
            Some(other) => Err(EvalError::mismatch("`not`", "a bool", other.type_name())),
            None => unreachable!("`exactly` returned one value"),
        },
        "exists" => match args.exactly(name, 1)?.pop() {
            Some(value) => Ok(EvalValue::Bool(value != EvalValue::Null)),
            None => unreachable!("`exactly` returned one value"),
        },
        _ => Err(EvalError::UnknownFunction(name.to_string())),
    }
}

fn truth(op: &str, value: EvalValue) -> Result<bool, EvalError> {
    match value {
        EvalValue::Bool(b) => Ok(b),
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
        // The right side is not evaluated once the left decides the answer.
        BinaryOp::And => {
            if !truth("&&", eval(lhs, cx, env)?)? {
                return Ok(EvalValue::Bool(false));
            }
            Ok(EvalValue::Bool(truth("&&", eval(rhs, cx, env)?)?))
        }
        BinaryOp::Or => {
            if truth("||", eval(lhs, cx, env)?)? {
                return Ok(EvalValue::Bool(true));
            }
            Ok(EvalValue::Bool(truth("||", eval(rhs, cx, env)?)?))
        }
        _ => {
            let (l, r) = (eval(lhs, cx, env)?, eval(rhs, cx, env)?);
            Ok(EvalValue::Bool(match op {
                BinaryOp::Eq => values_equal(&l, &r),
                BinaryOp::Ne => !values_equal(&l, &r),
                BinaryOp::Lt => compare(&l, &r)?.is_lt(),
                BinaryOp::Le => compare(&l, &r)?.is_le(),
                BinaryOp::Gt => compare(&l, &r)?.is_gt(),
                BinaryOp::Ge => compare(&l, &r)?.is_ge(),
                BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
            }))
        }
    }
}

/// The document a leading `#id` is resolved against: the one an address text
/// named (bound under [`address::ROOT`]), else the document being evaluated.
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

/// `#item(n)` and `doc#item(n)`: the n-th block at the root of the document.
/// `item` is the one id that is read as a position when it is called.
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
        Some(index) => nodes::root_item(doc, &index, cx),
        None => unreachable!("`exactly` returned one value"),
    }
}

/// `unicode(0x1F680)`, `unicode("U+1F680")`, `unicode("1F680")`: the character
/// with that code point.
fn unicode_char(code: &EvalValue) -> Result<EvalValue, EvalError> {
    let bad = || EvalError::message(format!("`unicode` expects a code point, got {code:?}"));
    let point = match code {
        EvalValue::Int(i) => u32::try_from(*i).map_err(|_| bad())?,
        EvalValue::String(text) => {
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
        .map(|c| EvalValue::String(c.to_string()))
        .ok_or_else(bad)
}
