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
/// `tomet_resolver::resolve_reference` (falling back to `@config` macros for bare identifiers);
/// a `Call` dispatches to builtins or `@config` user-defined macros after evaluating its args.
pub fn evaluate(doc: &Document, expr: &InterpExpr) -> Result<Value, ComputeError> {
    match &expr.kind {
        InterpExprKind::Literal(Literal::Int(i)) => Ok(Value::Int(*i)),
        InterpExprKind::Literal(Literal::Float(f)) => Ok(Value::Float(*f)),
        InterpExprKind::Literal(Literal::String(s)) => Ok(Value::String(s.clone())),
        InterpExprKind::Identifier(id) => {
            match tomet_resolver::resolve_reference(doc, expr) {
                Ok(val) => Ok(val),
                Err(err) => {
                    let config = tomet_semantics::document_config(doc);
                    if let Some(template) = config.macros.get(id) {
                        Ok(Value::String(template.clone()))
                    } else {
                        Err(err.into())
                    }
                }
            }
        }
        InterpExprKind::Member { .. } => {
            Ok(tomet_resolver::resolve_reference(doc, expr)?)
        }
        InterpExprKind::NamedArg { name, value } => {
            let val = evaluate(doc, value)?;
            Ok(Value::Map(vec![(name.clone(), val)]))
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
    match functions::call(name, &values) {
        Ok(val) => Ok(val),
        Err(ComputeError::UnknownFunction(_)) => {
            let config = tomet_semantics::document_config(doc);
            if let Some(template) = config.macros.get(name) {
                let expanded = expand_macro_template(template, &values);
                Ok(Value::String(expanded))
            } else {
                Err(ComputeError::UnknownFunction(name.to_string()))
            }
        }
        Err(e) => Err(e),
    }
}

fn expand_macro_template(template: &str, values: &[Value]) -> String {
    let mut result = template.to_string();

    // 1. Named placeholders: `${key}` from Map/NamedArg entries
    for val in values {
        if let Value::Map(entries) = val {
            for (k, v) in entries {
                let placeholder = format!("${{{k}}}");
                let val_str = value_to_string(v);
                result = result.replace(&placeholder, &val_str);
            }
        }
    }

    // 2. Positional placeholders: `${1}`, `${2}`, ...
    for (i, val) in values.iter().enumerate() {
        let val_str = match val {
            Value::Map(entries) if entries.len() == 1 => value_to_string(&entries[0].1),
            _ => value_to_string(val),
        };
        let brace_placeholder = format!("${{{}}}", i + 1);
        result = result.replace(&brace_placeholder, &val_str);
    }

    result
}

fn value_to_string(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
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

    #[test]
    fn evaluates_unicode_and_emoji() {
        let doc = parse("\n");
        assert_eq!(
            evaluate(&doc, &interp("$unicode(\"2713\")")).unwrap(),
            Value::String("✓".into())
        );
        assert_eq!(
            evaluate(&doc, &interp("$emoji(\"sparkles\")")).unwrap(),
            Value::String("✨".into())
        );
        assert_eq!(
            evaluate(&doc, &interp("$emoji(\"tada\")")).unwrap(),
            Value::String("🎉".into())
        );
    }

    #[test]
    fn evaluates_tm_and_ref_uris() {
        let doc = parse("\n");
        assert_eq!(
            evaluate(&doc, &interp("$tm(\"guide/intro\")")).unwrap(),
            Value::String("tm:guide/intro".into())
        );
        assert_eq!(
            evaluate(&doc, &interp("$tm(\"guide/intro\", \"installation\")")).unwrap(),
            Value::String("tm:guide/intro#installation".into())
        );
        assert_eq!(
            evaluate(&doc, &interp("$ref(\"アーキテクチャ\")")).unwrap(),
            Value::String("ref:アーキテクチャ".into())
        );
    }

    #[test]
    fn evaluates_date_formatting() {
        let doc = parse("\n");
        assert_eq!(
            evaluate(&doc, &interp("$date(\"2026-08-30\", \"YYYY年MM月DD日\")")).unwrap(),
            Value::String("2026年08月30日".into())
        );
        let date_result = evaluate(&doc, &interp("$date()")).unwrap();
        if let Value::String(s) = date_result {
            assert!(s.contains('-'));
        } else {
            panic!("expected date string");
        }
    }

    #[test]
    fn evaluates_user_defined_macros() {
        let doc = parse("@config{\n  macros: {\n    gh: \"https://github.com/tomet-lang/tomet/issues/${1}\"\n    greet: \"Hello, ${1} ${2}!\"\n    price: \"Price is $100 for ${1}\"\n    search: \"https://example.com/search?q=${q}&lang=${lang}\"\n    copyright: \"(C) 2026 Tomet Lang\"\n  }\n}\n");
        assert_eq!(
            evaluate(&doc, &interp("$gh(42)")).unwrap(),
            Value::String("https://github.com/tomet-lang/tomet/issues/42".into())
        );
        assert_eq!(
            evaluate(&doc, &interp("$greet(\"Alice\", \"Smith\")")).unwrap(),
            Value::String("Hello, Alice Smith!".into())
        );
        // Tests that currency `$100` in the template is preserved without being corrupted by `$1` matching
        assert_eq!(
            evaluate(&doc, &interp("$price(\"license\")")).unwrap(),
            Value::String("Price is $100 for license".into())
        );
        // Tests named arguments
        assert_eq!(
            evaluate(&doc, &interp("$search(q: \"rust\", lang: \"ja\")")).unwrap(),
            Value::String("https://example.com/search?q=rust&lang=ja".into())
        );
        assert_eq!(
            evaluate(&doc, &interp("${copyright}")).unwrap(),
            Value::String("(C) 2026 Tomet Lang".into())
        );
    }
}
