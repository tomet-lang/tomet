//! Evaluates a `${...}` interpolation's parsed `InterpExpr` --
//! `tomet-parser`/`tomet-ast` own the grammar and syntax tree;
//! this crate adds evaluation semantics on top. `Identifier`/`Member`
//! sub-expressions resolve via `tomet-resolver` (this crate has no id
//! lookup of its own); a `Call` dispatches to a builtin function
//! (`functions.rs`) after recursively evaluating its args.

mod error;
mod functions;

pub use error::ComputeError;

use tomet_ast::{Document, InterpExpr, InterpExprKind, Literal, Value};

/// Evaluates `expr` against `doc`. `Literal`s evaluate to themselves;
/// `Identifier`/`Member` chains resolve via
/// `tomet_resolver::resolve_reference`; a `Call`'s `args` are
/// evaluated recursively (left to right) before the named function runs.
pub fn evaluate(doc: &Document, expr: &InterpExpr) -> Result<Value, ComputeError> {
    match &expr.kind {
        InterpExprKind::Literal(Literal::Int(i)) => Ok(Value::Int(*i)),
        InterpExprKind::Literal(Literal::Float(f)) => Ok(Value::Float(*f)),
        InterpExprKind::Literal(Literal::String(s)) => Ok(Value::String(s.clone())),
        InterpExprKind::Identifier(_) | InterpExprKind::Member { .. } => {
            tomet_resolver::resolve_reference(doc, expr).map_err(ComputeError::Resolve)
        }
        InterpExprKind::Call { callee, args } => evaluate_call(doc, callee, args),
    }
}

fn evaluate_call(
    doc: &Document,
    callee: &InterpExpr,
    args: &[InterpExpr],
) -> Result<Value, ComputeError> {
    let InterpExprKind::Identifier(name) = &callee.kind else {
        return Err(ComputeError::UnsupportedCallee);
    };
    let values = args
        .iter()
        .map(|arg| evaluate(doc, arg))
        .collect::<Result<Vec<_>, _>>()?;
    functions::call(name, &values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Document {
        tomet_parser::parse_document(src).expect("valid Tomet source")
    }

    fn interp(src: &str) -> InterpExpr {
        let doc = parse(&format!("{src}\n"));
        match &doc.blocks[0] {
            tomet_ast::Block::Element(el) => match &el.value {
                Some(tomet_ast::ElementValue::Interp(expr)) => expr.clone(),
                other => panic!("expected ElementValue::Interp, got {other:?}"),
            },
            other => panic!("expected a standalone ${{...}} element, got {other:?}"),
        }
    }

    #[test]
    fn evaluates_literals() {
        let doc = parse("\n");
        assert_eq!(evaluate(&doc, &interp("${1}")).unwrap(), Value::Int(1));
        assert_eq!(
            evaluate(&doc, &interp("${1.5}")).unwrap(),
            Value::Float(1.5)
        );
        assert_eq!(
            evaluate(&doc, &interp("${\"hi\"}")).unwrap(),
            Value::String("hi".into())
        );
    }

    #[test]
    fn evaluates_identifier_via_resolve() {
        let doc = parse("<x>(id:a, n:5)\n");
        assert_eq!(evaluate(&doc, &interp("${a.n}")).unwrap(), Value::Int(5));
    }

    #[test]
    fn evaluates_int_arithmetic() {
        let doc = parse("\n");
        assert_eq!(
            evaluate(&doc, &interp("${add(1, 2)}")).unwrap(),
            Value::Int(3)
        );
        assert_eq!(
            evaluate(&doc, &interp("${sub(10, 3)}")).unwrap(),
            Value::Int(7)
        );
        assert_eq!(
            evaluate(&doc, &interp("${mul(3, 4)}")).unwrap(),
            Value::Int(12)
        );
        assert_eq!(
            evaluate(&doc, &interp("${mod(10, 3)}")).unwrap(),
            Value::Int(1)
        );
    }

    #[test]
    fn div_is_always_true_division() {
        let doc = parse("\n");
        assert_eq!(
            evaluate(&doc, &interp("${div(7, 2)}")).unwrap(),
            Value::Float(3.5)
        );
    }

    #[test]
    fn mixed_int_float_promotes_to_float() {
        let doc = parse("\n");
        assert_eq!(
            evaluate(&doc, &interp("${add(1.5, 2)}")).unwrap(),
            Value::Float(3.5)
        );
    }

    #[test]
    fn int_overflow_promotes_to_float_instead_of_panicking() {
        let doc = parse("\n");
        let src = format!("${{add({}, 1)}}", i64::MAX);
        let result = evaluate(&doc, &interp(&src)).unwrap();
        assert!(matches!(result, Value::Float(_)));
    }

    #[test]
    fn nested_calls_compose() {
        let doc = parse("\n");
        // add(mul(2, 3), 1) = 7
        assert_eq!(
            evaluate(&doc, &interp("${add(mul(2, 3), 1)}")).unwrap(),
            Value::Int(7)
        );
    }

    #[test]
    fn division_by_zero_is_an_error() {
        let doc = parse("\n");
        assert!(matches!(
            evaluate(&doc, &interp("${div(1, 0)}")).unwrap_err(),
            ComputeError::DivisionByZero
        ));
        assert!(matches!(
            evaluate(&doc, &interp("${mod(1, 0)}")).unwrap_err(),
            ComputeError::DivisionByZero
        ));
    }

    #[test]
    fn non_numeric_argument_is_an_error() {
        let doc = parse("\n");
        assert!(matches!(
            evaluate(&doc, &interp("${add(1, \"x\")}")).unwrap_err(),
            ComputeError::NotNumeric { .. }
        ));
    }

    #[test]
    fn unknown_function_is_an_error() {
        let doc = parse("\n");
        assert!(matches!(
            evaluate(&doc, &interp("${nope(1, 2)}")).unwrap_err(),
            ComputeError::UnknownFunction(name) if name == "nope"
        ));
    }

    #[test]
    fn wrong_arg_count_is_an_error() {
        let doc = parse("\n");
        assert!(matches!(
            evaluate(&doc, &interp("${add(1)}")).unwrap_err(),
            ComputeError::WrongArgCount {
                expected: 2,
                got: 1,
                ..
            }
        ));
    }

    #[test]
    fn callee_that_is_not_a_plain_identifier_is_an_error() {
        let doc = parse("<x>(id:a)\n");
        assert!(matches!(
            evaluate(&doc, &interp("${a.b(1, 2)}")).unwrap_err(),
            ComputeError::UnsupportedCallee
        ));
    }
}
