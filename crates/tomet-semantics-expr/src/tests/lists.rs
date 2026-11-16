//! Lists and the lambdas that go through them: `filter`, `sort`, `join`, `contains`.

use super::*;

#[test]
fn a_lambda_is_a_value_and_can_be_applied_by_a_method() {
    assert_eq!(
        with_var("var.xs.map(x => add(x, 1))", xs()).unwrap(),
        seq(vec![int(4), int(2), int(3)])
    );
}

#[test]
fn filter_keeps_what_the_function_says_yes_to() {
    assert_eq!(
        with_var("var.xs.filter(x => x > 1)", xs()).unwrap(),
        seq(vec![int(3), int(2)])
    );
}

#[test]
fn filter_needs_a_bool_from_its_function() {
    assert!(matches!(
        with_var("var.xs.filter(x => x)", xs()),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}

#[test]
fn sort_orders_by_a_key_and_is_stable() {
    assert_eq!(
        with_var("var.xs.sort(x => x)", xs()).unwrap(),
        seq(vec![int(1), int(2), int(3)])
    );
    assert_eq!(
        with_var("var.xs.sort(x => x, \"desc\")", xs()).unwrap(),
        seq(vec![int(3), int(2), int(1)])
    );
    // Equal keys keep their original order.
    let pairs = map(vec![(
        "xs",
        seq(vec![
            map(vec![("k", int(1)), ("n", text("a"))]),
            map(vec![("k", int(0)), ("n", text("b"))]),
            map(vec![("k", int(1)), ("n", text("c"))]),
        ]),
    )]);
    assert_eq!(
        with_var("var.xs.sort(p => p.k).map(p => p.n)", pairs).unwrap(),
        seq(vec![text("b"), text("a"), text("c")])
    );
}

#[test]
fn sort_refuses_keys_that_have_no_common_order() {
    let mixed = map(vec![("xs", seq(vec![int(1), text("a")]))]);
    assert_eq!(
        with_var("var.xs.sort(x => x)", mixed),
        Err(EvalError::NotComparable {
            left: "int",
            right: "string"
        })
    );
    assert!(with_var("var.xs.sort(x => x, \"sideways\")", xs()).is_err());
}

#[test]
fn an_item_with_no_key_goes_last_whichever_way_the_rest_are_ordered() {
    let xs = map(vec![(
        "xs",
        seq(vec![
            map(vec![("k", int(2)), ("n", text("a"))]),
            map(vec![
                ("k", EvalValue::Scalar(Scalar::None)),
                ("n", text("none1")),
            ]),
            map(vec![("k", int(1)), ("n", text("b"))]),
            map(vec![
                ("k", EvalValue::Scalar(Scalar::None)),
                ("n", text("none2")),
            ]),
        ]),
    )]);
    assert_eq!(
        with_var("var.xs.sort(p => p.k).map(p => p.n)", xs.clone()).unwrap(),
        seq(vec![text("b"), text("a"), text("none1"), text("none2")])
    );
    assert_eq!(
        with_var("var.xs.sort(p => p.k, \"desc\").map(p => p.n)", xs).unwrap(),
        seq(vec![text("a"), text("b"), text("none1"), text("none2")])
    );
    // Only keys that exist have to agree: a number and a string still clash.
    let clash = map(vec![(
        "xs",
        seq(vec![int(1), EvalValue::Scalar(Scalar::None), text("a")]),
    )]);
    assert!(with_var("var.xs.sort(x => x)", clash).is_err());
}

#[test]
fn join_len_contains_and_item_on_a_list() {
    assert_eq!(
        with_var("var.xs.join(\", \")", xs()).unwrap(),
        text("3, 1, 2")
    );
    assert_eq!(with_var("var.xs.len()", xs()).unwrap(), int(3));
    assert_eq!(
        with_var("var.xs.contains(2)", xs()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.xs.contains(9)", xs()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(with_var("var.xs.item(1)", xs()).unwrap(), int(1));
    assert!(with_var("var.xs.item(7)", xs()).is_err());
    assert!(with_var("var.xs.item(-1)", xs()).is_err());
}

#[test]
fn join_takes_scalars_only() {
    let nested = map(vec![("xs", seq(vec![seq(vec![int(1)])]))]);
    assert!(matches!(
        with_var("var.xs.join(\",\")", nested),
        Err(EvalError::TypeMismatch { got: "tuple", .. })
    ));
    let nulls = map(vec![("xs", seq(vec![EvalValue::Scalar(Scalar::None)]))]);
    assert!(with_var("var.xs.join(\",\")", nulls).is_err());
}

#[test]
fn an_empty_list_answers_contains_with_no_instead_of_failing() {
    // What a document with no tags gives for a `list` field.
    let empty = map(vec![("tags", seq(vec![]))]);
    assert_eq!(
        with_var("var.tags.contains(\"rust\")", empty).unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
}

#[test]
fn maps_and_strings_have_their_own_contains_and_get() {
    let m = map(vec![("m", map(vec![("a", int(1))]))]);
    assert_eq!(
        with_var("var.m.has(\"a\")", m.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.m.contains(\"a\")", m.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(with_var("var.m.get(\"a\", 0)", m.clone()).unwrap(), int(1));
    assert_eq!(with_var("var.m.get(\"z\", 0)", m).unwrap(), int(0));
    assert_eq!(
        out("\"hello\".contains(\"ell\")").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert!(matches!(
        out("\"hello\".contains(1)"),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
}
