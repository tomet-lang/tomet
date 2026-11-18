//! What a lambda sees, takes and is called with.

use super::*;

#[test]
fn a_lambda_sees_the_names_around_it() {
    assert_eq!(
        with_var("var.xs.map(x => var.xs.filter(y => y > x).len())", xs()).unwrap(),
        seq(vec![int(0), int(2), int(1)])
    );
}

#[test]
fn a_lambda_cannot_take_a_reserved_name_as_a_parameter() {
    for name in RESERVED_VALUES {
        let err = out(&format!("{name} => 1")).unwrap_err();
        assert!(err.to_string().contains("reserved"), "{name}: {err}");
    }
}

#[test]
fn a_lambda_called_with_the_wrong_number_of_arguments_is_reported() {
    assert!(matches!(
        with_var("var.xs.sort((a, b) => a)", xs()),
        Err(EvalError::WrongArgCount { .. })
    ));
}

#[test]
fn a_function_is_not_data() {
    assert_eq!(out("x => x").unwrap().to_ast(), None);
}
