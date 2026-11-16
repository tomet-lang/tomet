//! The arithmetic functions, and what an unknown or a retired function says.

use super::*;

#[test]
fn plus_and_minus_are_the_functions_add_and_sub() {
    assert_eq!(out("1 + 2").unwrap(), int(3));
    assert_eq!(out("5 - 7").unwrap(), int(-2));
    assert_eq!(
        out("1 + 1.5").unwrap(),
        EvalValue::Scalar(Scalar::Float(2.5))
    );
    assert_eq!(out("10 - 2 - 3").unwrap(), int(5));
    assert_eq!(
        out("1 + 2 == 3").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(out("add(1, 2) - 1").unwrap(), int(2));
    assert!(matches!(
        out("9223372036854775807 + 1").unwrap(),
        EvalValue::Scalar(Scalar::Float(_))
    ));
    assert!(matches!(
        out("1 + \"a\""),
        Err(EvalError::TypeMismatch { got: "string", .. })
    ));
}

#[test]
fn an_unknown_function_is_named_and_a_wrong_count_is_reported() {
    assert_eq!(
        out("frobnicate(1)"),
        Err(EvalError::UnknownFunction("frobnicate".to_string()))
    );
    assert!(matches!(
        out("add(1)"),
        Err(EvalError::WrongArgCount { got: 1, .. })
    ));
}

#[test]
fn the_old_comparison_and_logic_functions_are_gone() {
    for name in [
        "eq", "ne", "gt", "gte", "lt", "lte", "and", "or", "contains",
    ] {
        assert!(
            matches!(
                out(&format!("{name}(1, 2)")),
                Err(EvalError::UnknownFunction(_))
            ),
            "{name} should no longer be a function"
        );
    }
}
