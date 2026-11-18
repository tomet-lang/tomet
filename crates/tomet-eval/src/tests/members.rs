//! The members of a value: `.0`, a position, a signature alias, and a method on the wrong type.

use super::*;

#[test]
fn scalar_zero_member_and_tuple_operations() {
    // b.0 == b for each scalar, and not for a tuple, a call, None
    let vars = map(vec![
        ("s", text("hello")),
        ("i", int(42)),
        ("f", EvalValue::Scalar(Scalar::Float(1.5))),
        ("n", EvalValue::Scalar(Scalar::Number("1.10".into()))),
        ("b", EvalValue::Scalar(Scalar::Bool(true))),
        ("call", EvalValue::Call("func".into(), Tuple::new())),
        (
            "t_named",
            EvalValue::Tuple(Tuple::from_named([("k".into(), int(1))])),
        ),
    ]);

    assert_eq!(with_var("var.s.0", vars.clone()).unwrap(), text("hello"));
    assert_eq!(with_var("var.i.0", vars.clone()).unwrap(), int(42));
    assert_eq!(
        with_var("var.f.0", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Float(1.5))
    );
    assert_eq!(
        with_var("var.n.0", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Number("1.10".into()))
    );
    assert_eq!(
        with_var("var.b.0", vars.clone()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );

    // Non-zero member on scalar is an error
    assert_eq!(
        with_var("var.s.1", vars.clone()),
        Err(EvalError::UnknownMember {
            on: "string",
            member: "1".into()
        })
    );
    assert_eq!(
        with_var("var.i.k", vars.clone()),
        Err(EvalError::UnknownMember {
            on: "int",
            member: "k".into()
        })
    );

    // None, Call, and Tuple without "0" do NOT have .0
    assert_eq!(
        out("none.0"),
        Err(EvalError::UnknownMember {
            on: "none",
            member: "0".into()
        })
    );
    assert_eq!(
        with_var("var.call.0", vars.clone()),
        Err(EvalError::UnknownMember {
            on: "call",
            member: "0".into()
        })
    );
    assert_eq!(
        with_var("var.t_named.0", vars.clone()),
        Err(EvalError::UnknownMember {
            on: "tuple",
            member: "0".into()
        })
    );

    // use.0.0 and use.1.0 over ((a, version: 3), b, c)
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("test.tmt".into()),
        macros: &HashMap::new(),
        var: &EvalValue::Tuple(Tuple::new()),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let use_val = evaluate(
        &tomet_ast_parser::parse_interpolation_str(r#"{$ (("a", version: 3), "b", "c") }"#)
            .unwrap()
            .expr,
        &cx,
    )
    .unwrap();
    let cx_with_use = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("test.tmt".into()),
        macros: &HashMap::new(),
        var: &EvalValue::Tuple(Tuple::from_named([("use".into(), use_val)])),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };

    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.use.0.0 }")
                .unwrap()
                .expr,
            &cx_with_use
        )
        .unwrap(),
        text("a")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.use.1.0 }")
                .unwrap()
                .expr,
            &cx_with_use
        )
        .unwrap(),
        text("b")
    );

    // len, join, contains, map over (a, b, sep: ",") see two items and .sep reads ","
    let t_val = evaluate(
        &tomet_ast_parser::parse_interpolation_str(r#"{$ ("a", "b", sep: ",") }"#)
            .unwrap()
            .expr,
        &cx,
    )
    .unwrap();
    let cx_with_t = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("test.tmt".into()),
        macros: &HashMap::new(),
        var: &EvalValue::Tuple(Tuple::from_named([("t".into(), t_val)])),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };

    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.t.len() }")
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        int(2)
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.join("-") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text("a-b")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.contains("a") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.contains(",") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.has("sep") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.has("0") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.has(0) }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.t.sep }")
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text(",")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.get("sep", "default") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text(",")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.get("0", "default") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text("a")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.get(0, "default") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text("a")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str(r#"{$ var.t.get("unknown", "default") }"#)
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text("default")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.t.item(0) }")
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text("a")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.t.item(1) }")
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text("b")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.t.item(2) }")
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text(",")
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.t.positional().len() }")
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        int(2)
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.t.named().len() }")
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        int(0) // named() has no unnamed entries, so .len() on it is 0!
    );
    assert_eq!(
        evaluate(
            &tomet_ast_parser::parse_interpolation_str("{$ var.t.named().sep }")
                .unwrap()
                .expr,
            &cx_with_t
        )
        .unwrap(),
        text(",")
    );

    // Old or removed method on tuple returns UnknownMember with exact message
    let err = evaluate(
        &tomet_ast_parser::parse_interpolation_str("{$ var.t.keys() }")
            .unwrap()
            .expr,
        &cx_with_t,
    )
    .unwrap_err();
    assert_eq!(
        err,
        EvalError::UnknownMember {
            on: "tuple",
            member: "keys".to_string(),
        }
    );
    assert_eq!(err.to_string(), "a tuple has no member `keys`");
}

#[test]
fn addresses_by_position_and_signature_alias() {
    let doc_src = r#"::meta(title: Test)

@link("tm:foo")#(lk)
@heading(1)#(hd)
@custom("bar")#(cust)
"#;
    let resolver = MemoryResolver::new()
        .with_source("doc.tmt", doc_src)
        .unwrap();

    // 1. #lk.0 and #lk.target on @link give the exact same value
    assert_eq!(
        in_resolver(&resolver, "addr(tmt:doc.tmt#lk.0)").unwrap(),
        text("tm:foo")
    );
    assert_eq!(
        in_resolver(&resolver, "addr(tmt:doc.tmt#lk.target)").unwrap(),
        text("tm:foo")
    );
    assert_eq!(
        in_resolver(&resolver, "addr(tmt:doc.tmt#lk.0)").unwrap(),
        in_resolver(&resolver, "addr(tmt:doc.tmt#lk.target)").unwrap()
    );

    // 2. #hd.0 and #hd.level on @heading give the exact same value (level 1 from =[...])
    assert_eq!(
        in_resolver(&resolver, "addr(tmt:doc.tmt#hd.0)").unwrap(),
        int(1)
    );
    assert_eq!(
        in_resolver(&resolver, "addr(tmt:doc.tmt#hd.level)").unwrap(),
        int(1)
    );
    assert_eq!(
        in_resolver(&resolver, "addr(tmt:doc.tmt#hd.0)").unwrap(),
        in_resolver(&resolver, "addr(tmt:doc.tmt#hd.level)").unwrap()
    );

    // 3. Custom element without signature: #cust.0 gives "bar", #cust.target is UnknownMember
    assert_eq!(
        in_resolver(&resolver, "addr(tmt:doc.tmt#cust.0)").unwrap(),
        text("bar")
    );
    assert_eq!(
        in_resolver(&resolver, "addr(tmt:doc.tmt#cust.target)"),
        Err(EvalError::UnknownMember {
            on: "element",
            member: "target".to_string(),
        })
    );
}

#[test]
fn a_method_on_the_wrong_type_or_on_none_is_an_error() {
    assert_eq!(
        out("none.contains(1)"),
        Err(EvalError::UnknownMember {
            on: "none",
            member: "contains".to_string()
        })
    );
    assert_eq!(
        out("true.len()"),
        Err(EvalError::UnknownMember {
            on: "bool",
            member: "len".to_string()
        })
    );
}
