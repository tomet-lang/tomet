//! The evaluator's unit tests, by topic.

mod cache;
mod datetimes;
mod documents;
mod editor_names;
mod fragments;
mod functions;
mod include;
mod lambdas;
mod list_ids;
mod lists;
mod macros;
mod members;
mod names;
mod operators;
mod types;
mod values;

use std::collections::HashMap;

use tomet_ast::{Tuple, Value};
use tomet_expr::*;
use tomet_scalar::Scalar;
use tomet_type::{AutoFill, Datum, Kind, Type, TypeError};

use super::*;
use crate::evaluate_type;

fn int(i: i64) -> EvalValue {
    EvalValue::Scalar(Scalar::Int(i))
}

fn text(s: &str) -> EvalValue {
    EvalValue::Scalar(Scalar::Text(s.to_string()))
}

fn seq(items: Vec<EvalValue>) -> EvalValue {
    EvalValue::Tuple(Tuple::from_unnamed(items))
}

fn map(entries: Vec<(&str, EvalValue)>) -> EvalValue {
    EvalValue::Tuple(Tuple::from_named(
        entries.into_iter().map(|(k, v)| (k.to_string(), v)),
    ))
}

fn parse(src: &str) -> tomet_ast::InterpExpr {
    tomet_ast_parser::parse_interpolation_str(&format!("{{$ {src} }}"))
        .unwrap_or_else(|e| panic!("`{src}` does not parse: {e}"))
        .expr
}

fn run_with(
    src: &str,
    var: &EvalValue,
    macros: &HashMap<String, Value>,
) -> Result<EvalValue, EvalError> {
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros,
        var,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}

/// Evaluates with nothing in `var`.
fn out(src: &str) -> Result<EvalValue, EvalError> {
    run_with(src, &EvalValue::Scalar(Scalar::None), &HashMap::new())
}

/// Evaluates with `var` set.
fn with_var(src: &str, vars: EvalValue) -> Result<EvalValue, EvalError> {
    run_with(src, &vars, &HashMap::new())
}

fn xs() -> EvalValue {
    map(vec![("xs", seq(vec![int(3), int(1), int(2)]))])
}

const HERE: &str = "::kind(note)
::meta(
  title: Here
  tags: (a, b)
)

@card(n: 0)

=[ Intro ]#(intro)

Some body.

@card(n: 1, tags: (x, y, z)){ extra: yes }#(c1)
";

const DOC_A: &str = "::meta(
  title: A
)

@card(x: 7)#(top)
";

const DOC_B: &str = "::meta(
  title: B
)
";

fn vault() -> MemoryResolver {
    MemoryResolver::new()
        .with_source("here.tmt", HERE)
        .unwrap()
        .with_source("docs/a.tmt", DOC_A)
        .unwrap()
        .with_source("docs/sub/b.tmt", DOC_B)
        .unwrap()
}

/// Evaluates as written in `here.tmt` of the small vault above.
fn in_vault(src: &str) -> Result<EvalValue, EvalError> {
    in_resolver(&vault(), src)
}

/// Evaluates as written in `here.tmt` of `resolver`.
fn in_resolver(resolver: &MemoryResolver, src: &str) -> Result<EvalValue, EvalError> {
    let macros = HashMap::new();
    let vars = EvalValue::Scalar(Scalar::None);
    let cx = EvalContext {
        resolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}
