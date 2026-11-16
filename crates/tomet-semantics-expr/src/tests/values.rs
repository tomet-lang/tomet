//! Data and versions: `evaluate_data`, the order of versions, the number kept as written.

use super::*;

#[test]
fn evaluate_data_returns_ast_value_or_fails_when_not_storable() {
    let empty_map = HashMap::new();
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &empty_map,
        var: &EvalValue::Scalar(Scalar::None),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };

    let expr = parse("add(1, 2)");
    let data = evaluate_data(&expr, &cx).unwrap();
    assert_eq!(data, tomet_ast::Value::Scalar(Scalar::Int(3)));

    let fn_expr = parse("x => x");
    let err = evaluate_data(&fn_expr, &cx).unwrap_err();
    assert_eq!(
        err,
        EvalError::NotData {
            type_name: "function",
            is_vault_ref: false,
        }
    );
    assert_eq!(err.to_string(), "a function cannot be stored as data");
}

#[test]
fn a_list_of_documents_is_a_list_that_cannot_be_stored_as_data() {
    let resolver = vault();
    let macros = HashMap::new();
    let vars = EvalValue::Scalar(Scalar::None);
    let cx = EvalContext {
        resolver: &resolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &vars,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };

    // The message names the value's own type, as it did before `NotData` knew about vaults...
    let err = evaluate_data(&parse("vault::docs"), &cx).unwrap_err();
    assert_eq!(err.to_string(), "a tuple cannot be stored as data");
    // ...and a caller that has no vault can still tell that a vault is what the list is of.
    assert_eq!(
        err,
        EvalError::NotData {
            type_name: "tuple",
            is_vault_ref: true,
        }
    );
    let err = evaluate_data(&parse("self"), &cx).unwrap_err();
    assert_eq!(err.to_string(), "a document cannot be stored as data");
}

#[test]
fn versions_order_and_compare() {
    assert_eq!(
        out("version(\"1.10\") > version(\"1.9\")").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("version(\"1.10.0\") == version(\"1.10\")").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("version(\"2.0.0\") > version(\"1.99.9\")").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
}

#[test]
fn eval_value_number_preserves_written_representation_and_computes_numerically() {
    let ast_num = tomet_ast::Value::Scalar(Scalar::Number("1.10".into()));
    let eval_num = EvalValue::from_ast(&ast_num, &DocRef("test.tmt".into()));
    assert_eq!(eval_num, EvalValue::Scalar(Scalar::Number("1.10".into())));
    assert_eq!(eval_num.scalar_text().as_deref(), Some("1.10"));
    assert_eq!(eval_num.type_name(), "number");

    let vars = map(vec![(
        "rev",
        EvalValue::Scalar(Scalar::Number("1.10".into())),
    )]);

    // Interpolation / expression value access preserves exact text
    assert_eq!(
        with_var("var.rev", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Number("1.10".into()))
    );
    assert_eq!(
        with_var("var.rev", vars.clone())
            .unwrap()
            .scalar_text()
            .as_deref(),
        Some("1.10")
    );

    // Arithmetic via add / sub / mul
    assert_eq!(
        with_var("add(var.rev, 1)", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Float(2.1))
    );
    assert_eq!(
        with_var("add(1, var.rev)", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Float(2.1))
    );
    assert_eq!(
        with_var("sub(var.rev, 0.1)", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Float(1.0))
    );
    assert_eq!(
        with_var("mul(var.rev, 2)", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Float(2.2))
    );

    // Equality
    assert_eq!(
        with_var("var.rev == 1.1", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.rev == 1.10", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.rev != 1.2", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );

    // Ordering
    assert_eq!(
        with_var("var.rev > 1.0", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.rev < 1.2", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.rev >= 1.1", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.rev <= 1.1", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );

    // Sorting in methods
    let list_vars = map(vec![(
        "items",
        seq(vec![
            EvalValue::Scalar(Scalar::Number("1.10".into())),
            EvalValue::Scalar(Scalar::Int(2)),
            EvalValue::Scalar(Scalar::Float(0.5)),
        ]),
    )]);
    assert_eq!(
        with_var("var.items.sort(x => x)", list_vars).unwrap(),
        seq(vec![
            EvalValue::Scalar(Scalar::Float(0.5)),
            EvalValue::Scalar(Scalar::Number("1.10".into())),
            EvalValue::Scalar(Scalar::Int(2)),
        ])
    );
}
