//! Equality, ordering, `and`, `or`, `not`, `exists` and their precedence.

use super::*;

#[test]
fn ordering_takes_two_numbers_or_two_strings_and_nothing_else() {
    assert_eq!(out("1 < 2").unwrap(), EvalValue::Scalar(Scalar::Bool(true)));
    assert_eq!(
        out("2 <= 2").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("1.5 > 1").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("\"2026-01-02\" >= \"2026-01-01\"").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("1 < \"a\""),
        Err(EvalError::NotComparable {
            left: "int",
            right: "string"
        })
    );
    assert_eq!(
        out("none < 1"),
        Err(EvalError::NotComparable {
            left: "none",
            right: "int"
        })
    );
}

#[test]
fn and_or_skip_the_right_side_once_the_left_decides() {
    // The right side names something that does not exist: it would fail if run.
    assert_eq!(
        out("false && nope").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(
        out("true || nope").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("true && false").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(
        out("false || true").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert!(out("true && nope").is_err());
    assert!(out("false || nope").is_err());
}

#[test]
fn and_or_take_only_booleans() {
    assert!(matches!(
        out("1 && true"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert!(matches!(
        out("none || true"),
        Err(EvalError::TypeMismatch { got: "none", .. })
    ));
    assert!(matches!(
        out("true && \"x\""),
        Err(EvalError::TypeMismatch { got: "string", .. })
    ));
}

#[test]
fn not_and_exists() {
    assert_eq!(
        out("not(true)").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert!(matches!(
        out("not(0)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert_eq!(
        out("exists(none)").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(
        out("exists(0)").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("exists(\"\")").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
}

#[test]
fn precedence_puts_comparison_before_and_before_or() {
    assert_eq!(
        out("1 < 2 && 3 > 2 || false").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("false && true || true").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("false && (true || true)").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
}

#[test]
fn arithmetic_functions_keep_integers_and_promote_on_overflow() {
    assert_eq!(out("add(1, 2)").unwrap(), int(3));
    assert_eq!(out("sub(5, 7)").unwrap(), int(-2));
    assert_eq!(out("mul(3, 4)").unwrap(), int(12));
    assert_eq!(
        out("add(1, 1.5)").unwrap(),
        EvalValue::Scalar(Scalar::Float(2.5))
    );
    assert_eq!(
        out("div(7, 2)").unwrap(),
        EvalValue::Scalar(Scalar::Float(3.5))
    );
    assert_eq!(out("mod(7, 3)").unwrap(), int(1));
    assert!(matches!(
        out("add(9223372036854775807, 1)").unwrap(),
        EvalValue::Scalar(Scalar::Float(_))
    ));
    assert_eq!(out("div(1, 0)"), Err(EvalError::DivisionByZero));
    assert_eq!(out("mod(1, 0)"), Err(EvalError::DivisionByZero));
    assert!(matches!(
        out("add(1, \"a\")"),
        Err(EvalError::TypeMismatch { got: "string", .. })
    ));
}
