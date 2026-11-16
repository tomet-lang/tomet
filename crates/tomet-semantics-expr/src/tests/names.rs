//! Names, reserved values and `var`.

use super::*;

#[test]
fn literals_evaluate_to_themselves() {
    assert_eq!(out("1").unwrap(), int(1));
    assert_eq!(out("1.5").unwrap(), EvalValue::Scalar(Scalar::Float(1.5)));
    assert_eq!(out("-3").unwrap(), int(-3));
    assert_eq!(out("\"hi\"").unwrap(), text("hi"));
    assert_eq!(out("true").unwrap(), EvalValue::Scalar(Scalar::Bool(true)));
    assert_eq!(out("none").unwrap(), EvalValue::Scalar(Scalar::None));
    assert_eq!(out("auto").unwrap(), EvalValue::Scalar(Scalar::Auto));
    assert!(tomet_ast_parser::parse_interpolation_str("{$ null }").is_err());
    assert_eq!(out("(1)").unwrap(), int(1));
    assert_eq!(out("()").unwrap(), seq(vec![]));
    assert_eq!(out("(\"a\",)").unwrap(), seq(vec![text("a")]));
    assert_eq!(
        out("(\"a\", \"b\")").unwrap(),
        seq(vec![text("a"), text("b")])
    );
    assert_eq!(out("(\"a\", \"b\").len()").unwrap(), int(2));
    assert_eq!(out("(\"a\", \"b\").join(\", \")").unwrap(), text("a, b"));
}

#[test]
fn an_unknown_name_is_an_error_and_never_falls_through() {
    assert_eq!(
        out("intro"),
        Err(EvalError::UnknownName("intro".to_string()))
    );
    assert_eq!(out("gh"), Err(EvalError::UnknownName("gh".to_string())));
}

#[test]
fn reserved_values_resolve() {
    assert_eq!(
        out("self").unwrap(),
        EvalValue::Document(DocRef("here.tmt".into()))
    );
    assert_eq!(
        out("vault").unwrap(),
        EvalValue::Directory(DirRef(String::new()))
    );
}

#[test]
fn var_is_the_variables_in_scope() {
    let var = map(vec![("title", text("T"))]);
    assert_eq!(with_var("var.title", var).unwrap(), text("T"));
}

#[test]
fn generate_is_not_a_name() {
    // The namespace is gone: nothing reserves it, and nothing answers to it.
    assert_eq!(
        out("generate"),
        Err(EvalError::UnknownName("generate".to_string()))
    );
    assert!(out("generate.uuid()").is_err());
    assert!(!RESERVED_VALUES.contains(&"generate"));
    assert!(RESERVED_NAMES.iter().all(|(name, _)| *name != "generate"));
}

#[test]
fn a_declared_variable_with_no_value_says_how_to_give_it_one() {
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &HashMap::new(),
        var: &map(vec![("other", text("O"))]),
        var_unset: &["title".to_string()],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    assert_eq!(
        evaluate(&parse("var.title"), &cx),
        Err(EvalError::NoValue("title".to_string()))
    );
    // a name that is not declared at all is still an unknown member
    assert_eq!(
        evaluate(&parse("var.nope"), &cx),
        Err(EvalError::UnknownMember {
            on: "tuple",
            member: "nope".to_string()
        })
    );
    assert_eq!(evaluate(&parse("var.other"), &cx).unwrap(), text("O"));
}

#[test]
fn a_member_that_does_not_exist_is_named() {
    let vars = map(vec![("title", text("T"))]);
    assert_eq!(
        with_var("var.nope", vars),
        Err(EvalError::UnknownMember {
            on: "tuple",
            member: "nope".to_string()
        })
    );
    assert_eq!(
        out("true.foo"),
        Err(EvalError::UnknownMember {
            on: "bool",
            member: "foo".to_string()
        })
    );
}

#[test]
fn equality_compares_numbers_as_numbers_and_never_errors_across_types() {
    assert_eq!(
        out("1 == 1.0").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("1 != 2").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("\"a\" == \"a\"").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("1 == \"1\"").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(
        out("none == true").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(
        out("none == none").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
}
