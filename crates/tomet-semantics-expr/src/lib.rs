//! =[ tomet-expr ]
//!
//! Expression evaluation for the `${...}` expressions of a Tomet document
//! ([`InterpExpr`]), following `tmtroot/spec/expressions/`.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Pure Boundary (Zero I/O))
//!   | Evaluation performs no I/O and reads no clock. What it cannot know by
//!   | itself -- another document, a directory, the vault's settings, the
//!   | time -- comes through [`Resolver`] and [`Generate`], which the layers
//!   | above implement.
//! - (Typed values)
//!   | [`EvalValue`] is not the `.tmt` data model ([`tomet_ast::Value`]): it
//!   | also holds documents, directories, functions and datetimes, which can
//!   | be the result of an expression and never data. A fragment (`link`,
//!   | `include`) is the one non-scalar value that can be placed in output.
//! - (Names by position)
//!   | A name is looked up in the one table its position selects: a call in
//!   | the builtin functions, a bare name in the lambda parameters and then
//!   | the reserved values ([`RESERVED_VALUES`]). There is no fall through,
//!   | and an unknown name is an error.
//! - (No conversions)
//!   | Operators and methods use the values they are given and convert none
//!   | of them. `&&` and `||` skip their right side once the left decides,
//!   | and take booleans only.
//! - (When it runs)
//!   | [`Phase::Output`] is rendering; the same expression gives the same
//!   | result every time. [`Phase::Create`] is making a document from a
//!   | blueprint, and is the only place `generate.*` exists. `var` exists in both:
//!   | the variables a document or a vault declares, filled in by `tomet new --var`
//!   | when creating.
//! - (Macros)
//!   | Macros defined by `::macros` are called as `macro.name(...)`. Macro
//!   | arguments are ordinary expressions. Calls check argument counts and
//!   | required named arguments, reporting `unknown macro` when missing.
//!
//! The pass that puts the values back into a document is
//! `tomet_transform::resolve_expressions`.

mod address;
mod args;
mod chars;
mod context;
mod datetime;
mod error;
mod fragments;
mod include;
mod memory;
mod methods;
mod nodes;
mod ops;
mod value;
mod vars;

#[cfg(test)]
mod tests;

pub use context::{
    EvalContext, Generate, NoResolver, Note, Notes, OffsetGenerate, Phase, PreparedBlocks,
    Resolver, SystemGenerate,
};
pub use datetime::{format_datetime, parse_datetime, parse_offset};
pub use error::EvalError;
pub use include::MAX_INCLUDE_DEPTH;
pub use memory::MemoryResolver;
pub use value::{Closure, DirRef, DocRef, Env, EvalValue, Namespace, Node};
pub use vars::{VarError, VarScope, shaped, typed_value, var_scope};

use tomet_ast::{BinaryOp, InterpExpr, InterpExprKind, Literal};

use args::Args;
use ops::{arithmetic, compare, values_equal};

/// The names that stand for a value and that no lambda parameter may take.
/// `generate` exists only while a document is being created.
pub const RESERVED_VALUES: [&str; 5] = ["self", "macro", "var", "vault", "generate"];

/// Evaluates `expr` in `cx`.
pub fn evaluate(expr: &InterpExpr, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    eval(expr, cx, &Env::new())
}

/// Whether `value` is, or holds, a document, a directory or an element: something that
/// is found through a vault and has no data form of its own.
fn refers_to_vault(value: &EvalValue) -> bool {
    match value {
        EvalValue::Document(_) | EvalValue::Directory(_) | EvalValue::Node { .. } => true,
        EvalValue::Seq(items) => items.iter().any(refers_to_vault),
        EvalValue::Map(entries) => entries.iter().any(|(_, v)| refers_to_vault(v)),
        EvalValue::Call(_, args) => args.iter().any(refers_to_vault),
        _ => false,
    }
}

/// Evaluates `expr` in `cx`, requiring the result to have a data representation.
///
/// If evaluation succeeds but the value cannot be stored as data, an [`EvalError::NotData`]
/// is returned.
pub fn evaluate_data(expr: &InterpExpr, cx: &EvalContext) -> Result<tomet_ast::Value, EvalError> {
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
            // A variable that is declared and has no value is not in the map, so
            // it would read as an unknown member. It is a different mistake: say so.
            if matches!(&object.kind, InterpExprKind::Identifier(name) if name == "var")
                && env.get("var").is_none()
                && cx.var_unset.iter().any(|unset| unset == member)
            {
                return Err(EvalError::NoValue(member.clone()));
            }
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
        "var" => Ok(cx.var.clone()),
        "generate" if cx.phase == Phase::Output => Err(EvalError::PhaseOnly(name.to_string())),
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
        EvalValue::DateTime(value) => datetime::member(value, member),
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

fn is_ident_placeholder(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_alphabetic() || first == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// `macro.name(args)`: the template with its placeholders filled in.
/// Positional placeholders `${1}`, `${2}`, ... take positional arguments, and
/// named placeholders `${ident}` take named arguments. Any other `${...}` in
/// the template is treated as plain text.
///
/// Arguments are ordinary expressions. Missing or excess arguments report arity
/// errors, missing named arguments report the missing name, and an unknown
/// macro reports `unknown macro`.
fn macro_call(name: &str, args: Args, cx: &EvalContext) -> Result<EvalValue, EvalError> {
    let template = cx
        .macros
        .get(name)
        .ok_or_else(|| EvalError::UnknownFunction(format!("macro.{name}")))?;

    let mut max_pos = 0;
    let mut named_keys = Vec::new();
    let mut rest = template.as_str();
    while let Some(start) = rest.find("${") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find('}') {
            let key = &rest[..end];
            if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(idx) = key.parse::<usize>() {
                    max_pos = max_pos.max(idx);
                }
            } else if is_ident_placeholder(key) && !named_keys.contains(&key.to_string()) {
                named_keys.push(key.to_string());
            }
            rest = &rest[end + 1..];
        }
    }

    if max_pos > 0 && args.positional.len() != max_pos {
        return Err(EvalError::WrongArgCount {
            function: format!("macro.{name}"),
            expected: max_pos.to_string(),
            got: args.positional.len(),
        });
    }
    if max_pos == 0
        && named_keys.is_empty()
        && (!args.positional.is_empty() || !args.named.is_empty())
    {
        return Err(EvalError::WrongArgCount {
            function: format!("macro.{name}"),
            expected: "0".to_string(),
            got: args.positional.len() + args.named.len(),
        });
    }
    for expected_key in &named_keys {
        if !args.named.iter().any(|(k, _)| k == expected_key) {
            return Err(EvalError::message(format!(
                "macro.{name} expects named argument `{expected_key}`"
            )));
        }
    }

    let mut text = template.clone();
    for (key, value) in &args.named {
        if is_ident_placeholder(key) {
            text = text.replace(&format!("${{{key}}}"), &methods::scalar_text(value)?);
        }
    }
    for (i, value) in args.positional.iter().enumerate() {
        text = text.replace(&format!("${{{}}}", i + 1), &methods::scalar_text(value)?);
    }
    Ok(EvalValue::String(text))
}

/// The functions an expression can call by a bare name, each with a line
/// saying what it does -- what an editor offers when `${` is typed.
///
/// `addr` is read by the parser rather than called, and is listed because it
/// is written like a call. Every other name is one [`builtin`] answers, which
/// a test holds to; a function added there is added here, so the editor does
/// not offer the language of last month.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("tmt", "A document by its path `tmt(\"path\")`"),
    ("ref", "A document by its note name `ref(\"name\")`"),
    (
        "addr",
        "An address written as text `addr(tmt:docs/a.tmt#intro)`",
    ),
    (
        "include",
        "A document's content, put here `include(tmt(\"path\"))`",
    ),
    ("link", "A link to a document or an element `link(target)`"),
    ("add", "Arithmetic addition function `add(a, b)`"),
    ("sub", "Arithmetic subtraction function `sub(a, b)`"),
    ("mul", "Arithmetic multiplication function `mul(a, b)`"),
    ("div", "Floating-point division function `div(a, b)`"),
    ("mod", "Integer modulo function `mod(a, b)`"),
    ("not", "Negation `not(bool)`"),
    ("exists", "Whether a value is not null `exists(value)`"),
    ("emoji", "An emoji by name `emoji(\"sparkles\")`"),
    ("unicode", "A character by code point `unicode(0x2713)`"),
];

/// The names an expression can start with that are values, not functions,
/// each with a line saying what it is.
pub const RESERVED_NAMES: &[(&str, &str)] = &[
    ("self", "The document this expression is written in"),
    (
        "macro",
        "The macros defined by `::macros`: `macro.name(...)`",
    ),
    (
        "var",
        "The variables declared by `::vars`: `var.name` (`tomet new --var` fills them in)",
    ),
    ("vault", "The vault's root directory: `vault.docs`"),
    (
        "generate",
        "`generate.uuid()` and `generate.now()` (only when creating a document)",
    ),
];

/// The functions that are called by a bare name. Keep [`FUNCTIONS`] in step.
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
        "include" => include::include(args, cx),
        "link" => match args.exactly(name, 1)?.pop() {
            Some(target) => fragments::link_fragment(&target),
            None => unreachable!("`exactly` returned one value"),
        },
        "emoji" => match args.exactly(name, 1)?.pop() {
            Some(EvalValue::String(name)) => {
                let key = name.trim().trim_matches(':');
                chars::emoji_for(key)
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
