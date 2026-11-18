//! Types are values: type names, calls of them, `auto` and the type seat.

use super::*;

/// The type written as text (`list(string, length: 6)`), as a value.
fn ty(written: &str) -> EvalValue {
    EvalValue::Type(
        written
            .parse()
            .unwrap_or_else(|e| panic!("`{written}`: {e}")),
    )
}

/// What a type seat holds, when `src` is written in one.
fn seat(src: &str) -> Result<Type, EvalError> {
    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &HashMap::new(),
        var: &EvalValue::Scalar(Scalar::None),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate_type(&parse(src), &cx)
}

fn bad(error: TypeError) -> EvalError {
    EvalError::BadType(error)
}

#[test]
fn a_bare_type_name_is_that_type() {
    for kind in Kind::ALL {
        assert_eq!(
            out(kind.name()).unwrap(),
            EvalValue::Type(Type::bare(kind)),
            "{kind}"
        );
    }
    assert_eq!(out("list").unwrap().type_name(), "type");
}

#[test]
fn a_lambda_parameter_comes_before_a_type_name() {
    // `int` is the parameter here, so the numbers come back as they are.
    assert_eq!(
        with_var("var.xs.map(int => int)", xs()).unwrap(),
        seq(vec![int(3), int(1), int(2)])
    );
    assert_eq!(
        with_var("var.xs.map(x => int)", xs()).unwrap(),
        seq(vec![ty("int"), ty("int"), ty("int")])
    );
}

#[test]
fn a_call_of_a_type_name_builds_the_type() {
    assert_eq!(out("list(string)").unwrap(), ty("list(string)"));
    assert_eq!(
        out("list(string, length: 6)").unwrap(),
        ty("list(string, length: 6)")
    );
    assert_eq!(
        out("list(string, default: (\"a\", \"b\"))").unwrap(),
        ty("list(string, default: (\"a\", \"b\"))")
    );
    assert_eq!(
        out("list(string, default: ())").unwrap(),
        ty("list(string, default: ())")
    );
    assert_eq!(
        out("list(string, default: (\"a\",))").unwrap(),
        ty("list(string, default: (\"a\",))")
    );
    assert_eq!(
        out("list(string, default: (\"a\"))").unwrap(),
        ty("list(string, default: (\"a\",))")
    );
    assert_eq!(
        out("nanoid(length: 8, prefix: \"note-\", default: auto)").unwrap(),
        ty("nanoid(length: 8, prefix: \"note-\", default: auto)")
    );
    assert_eq!(
        out("date(default: auto)").unwrap(),
        ty("date(default: auto)")
    );
    assert_eq!(
        out("uuid(prefix: \"id-\")").unwrap(),
        ty("uuid(prefix: \"id-\")")
    );
    assert_eq!(
        out("string(default: \"x\")").unwrap(),
        ty("string(default: \"x\")")
    );
    assert_eq!(out("int(default: 3)").unwrap(), ty("int(default: 3)"));
    // A type is its arguments, however they were written.
    assert_eq!(
        out("list(string, length: 6)").unwrap(),
        out("list(length: 6, string)").unwrap()
    );
}

#[test]
fn types_nest() {
    assert_eq!(
        out("list(list(string), length: 6)").unwrap(),
        ty("list(list(string), length: 6)")
    );
    assert_eq!(
        out("list(list(list(int)))").unwrap(),
        ty("list(list(list(int)))")
    );
}

#[test]
fn a_function_wins_over_a_type_of_the_same_name() {
    // `version(...)` is the function; `version` alone is the type.
    assert_eq!(
        out("version(\"1.10\")").unwrap(),
        EvalValue::Version(tomet_type::Version::parse("1.10").unwrap())
    );
    assert_eq!(out("version").unwrap(), ty("version"));
    // So a type of that name cannot be built with arguments in an expression.
    assert!(matches!(
        out("version(default: \"1.0\")"),
        Err(EvalError::Message(_))
    ));
}

#[test]
fn auto_is_read_as_the_default_of_a_type_and_a_keyword_value() {
    assert_eq!(out("auto").unwrap(), EvalValue::Scalar(Scalar::Auto));
    assert!(matches!(
        out("add(1, auto)"),
        Err(EvalError::TypeMismatch { got: "auto", .. })
    ));
    // Not as another argument of a type, nor by position.
    assert_eq!(
        out("nanoid(length: auto)"),
        Err(EvalError::BadType(TypeError::AutoElsewhere {
            ty: Kind::Nanoid,
            argument: "length".to_string(),
        }))
    );
    assert_eq!(
        out("list(auto)"),
        Err(EvalError::BadType(TypeError::AutoElsewhere {
            ty: Kind::List,
            argument: "item".to_string(),
        }))
    );
    // Not as the default of anything but a type.
    assert_eq!(
        out("add(default: auto)"),
        Err(EvalError::Message(
            "`add` takes no named argument, got `default`".into()
        ))
    );
    // As the default of a type it is the type's `auto`.
    let EvalValue::Type(made) = out("uuid(default: auto)").unwrap() else {
        panic!("a type");
    };
    assert_eq!(made.default(), Some(&Datum::Scalar(Scalar::Auto)));
    assert_eq!(made.auto(), Some(AutoFill::Id));
}

#[test]
fn auto_is_admitted_for_the_kinds_that_make_a_value_up_and_refused_for_the_rest() {
    // `version` is a function in an expression, and its arguments are values; a seat has it as a
    // type (`a_type_seat_admits_auto_only_for_the_kinds_that_make_a_value_up`).
    assert_eq!(
        out("version(default: auto)"),
        Err(EvalError::Message(
            "`version` takes no named argument, got `default`".into()
        ))
    );
    for kind in Kind::ALL
        .into_iter()
        .filter(|kind| kind.name() != "version")
    {
        let written = format!("{kind}(default: auto)");
        match kind.auto() {
            Some(_) => assert!(
                matches!(out(&written), Ok(EvalValue::Type(made)) if made.kind() == kind),
                "{written}"
            ),
            None => assert_eq!(
                out(&written),
                Err(bad(TypeError::AutoNotAllowed { ty: kind })),
                "{written}"
            ),
        }
    }
}

#[test]
fn a_type_that_is_not_well_formed_is_refused_in_the_type_crates_words() {
    let refused = [
        (
            "list(string, string)",
            TypeError::TooManyArguments {
                ty: Kind::List,
                takes: 1,
                given: 2,
            },
        ),
        (
            "string(prefix: \"x\")",
            TypeError::UnknownArgument {
                ty: Kind::String,
                argument: "prefix".to_string(),
            },
        ),
        (
            "nanoid(length: 4, length: 5)",
            TypeError::DuplicateArgument {
                ty: Kind::Nanoid,
                argument: "length".to_string(),
            },
        ),
    ];
    for (written, error) in refused {
        let got = out(written);
        assert_eq!(got, Err(bad(error.clone())), "{written}");
        // The message is the type crate's own, not a paraphrase of it.
        assert_eq!(got.unwrap_err().to_string(), error.to_string());
    }
    assert!(matches!(
        out("nanoid(length: 0)"),
        Err(EvalError::BadType(TypeError::WrongArgument {
            ty: Kind::Nanoid,
            argument: "length",
            ..
        }))
    ));
    assert!(matches!(
        out("list(1)"),
        Err(EvalError::BadType(TypeError::WrongArgument {
            ty: Kind::List,
            argument: "item",
            ..
        }))
    ));
}

#[test]
fn an_unknown_type_name_is_not_a_type() {
    assert_eq!(
        out("person"),
        Err(EvalError::UnknownName("person".to_string()))
    );
    assert_eq!(
        out("person(1)"),
        Err(EvalError::UnknownFunction("person".to_string()))
    );
}

#[test]
fn an_argument_of_a_type_is_a_type_or_plain_data() {
    // A number that was kept as written is the number it is, and a version is its text.
    let vars = map(vec![
        ("n", EvalValue::Scalar(Scalar::Number("8".to_string()))),
        (
            "v",
            EvalValue::Version(tomet_type::Version::parse("1.2").unwrap()),
        ),
        (
            "doc",
            seq(vec![EvalValue::Document(DocRef("a.tmt".into()))]),
        ),
    ]);
    assert_eq!(
        with_var("nanoid(length: var.n)", vars.clone()).unwrap(),
        ty("nanoid(length: 8)")
    );
    assert_eq!(
        with_var("string(default: var.v)", vars.clone()).unwrap(),
        ty("string(default: \"1.2\")")
    );
    // Documents, functions and the like are not plain data.
    for written in [
        "nanoid(length: self)",
        "nanoid(length: x => x)",
        "string(default: vault)",
        "string(default: var.doc)",
    ] {
        let err = with_var(written, vars.clone()).unwrap_err();
        assert!(
            matches!(&err, EvalError::TypeMismatch { what, .. } if what.starts_with("an argument of the type")),
            "{written}: {err:?}"
        );
        assert!(err.to_string().contains("plain data"), "{err}");
    }
}

#[test]
fn types_are_equal_when_they_are_the_same_type_and_have_no_order() {
    assert_eq!(
        out("list(string) == list(string)").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        out("list(string) == list(string, length: 2)").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(
        out("string != int").unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    // Not the type's name as text, and not the text form.
    assert_eq!(
        out("string == \"string\"").unwrap(),
        EvalValue::Scalar(Scalar::Bool(false))
    );
    assert_eq!(
        out("string < int"),
        Err(EvalError::NotComparable {
            left: "type",
            right: "type"
        })
    );
    assert_eq!(
        out("string < 1"),
        Err(EvalError::NotComparable {
            left: "type",
            right: "int"
        })
    );
}

#[test]
fn a_type_is_not_data_and_not_text() {
    let made = out("list(string)").unwrap();
    assert_eq!(made.to_ast(), None);
    assert_eq!(made.scalar_text(), None);
    // One anywhere inside makes the whole thing not data.
    assert_eq!(seq(vec![int(1), made.clone()]).to_ast(), None);

    let cx = EvalContext {
        resolver: &NoResolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &HashMap::new(),
        var: &EvalValue::Scalar(Scalar::None),
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    let err = evaluate_data(&parse("list(string)"), &cx).unwrap_err();
    assert_eq!(
        err,
        EvalError::NotData {
            type_name: "type",
            is_vault_ref: false
        }
    );
    assert_eq!(err.to_string(), "a type cannot be stored as data");
}

#[test]
fn a_type_has_no_members() {
    assert_eq!(
        out("list(string).length"),
        Err(EvalError::UnknownMember {
            on: "type",
            member: "length".to_string()
        })
    );
    assert!(out("string.len()").is_err());
}

#[test]
fn a_type_seat_holds_the_type_it_is_written_as() {
    for kind in Kind::ALL {
        assert_eq!(seat(kind.name()), Ok(Type::bare(kind)), "{kind}");
    }
    assert_eq!(seat("list(string, length: 6)").unwrap().length(), Some(6));
    assert_eq!(
        seat("nanoid(length: 8, prefix: \"note-\", default: auto)"),
        Ok("nanoid(length: 8, prefix: \"note-\", default: auto)"
            .parse()
            .unwrap())
    );
    assert_eq!(
        seat("list(list(string), length: 6)"),
        Ok("list(list(string), length: 6)".parse().unwrap())
    );
    assert_eq!(
        seat("date(default: auto)").unwrap().auto(),
        Some(AutoFill::Now)
    );
}

#[test]
fn a_type_seat_reads_the_types_first() {
    // Where an expression has a function `version(...)`, a seat has the type.
    assert_eq!(seat("version"), Ok(Type::bare(Kind::Version)));
    assert_eq!(
        seat("version(default: \"1.0\")"),
        Ok("version(default: \"1.0\")".parse().unwrap())
    );
    assert_eq!(
        seat("version(\"1.10\")"),
        Err(bad(TypeError::TooManyArguments {
            ty: Kind::Version,
            takes: 0,
            given: 1
        }))
    );
    assert_eq!(seat("list(version)"), Ok("list(version)".parse().unwrap()));
}

#[test]
fn a_type_seat_admits_auto_only_for_the_kinds_that_make_a_value_up() {
    for kind in Kind::ALL {
        let written = format!("{kind}(default: auto)");
        match kind.auto() {
            Some(fill) => assert_eq!(seat(&written).unwrap().auto(), Some(fill), "{written}"),
            None => assert_eq!(
                seat(&written),
                Err(bad(TypeError::AutoNotAllowed { ty: kind })),
                "{written}"
            ),
        }
    }
    // And `auto` is nothing outside `default`.
    assert_eq!(
        seat("nanoid(length: auto)"),
        Err(bad(TypeError::AutoElsewhere {
            ty: Kind::Nanoid,
            argument: "length".to_string(),
        }))
    );
    assert_eq!(
        seat("auto"),
        Err(EvalError::Message(
            "a type seat holds a type, and `auto` is a auto".into()
        ))
    );
}

#[test]
fn a_type_seat_with_an_unknown_name_or_bad_arguments_is_refused() {
    // What a seat holds is a type, so an unknown name is an unknown type, in the type crate's words.
    assert_eq!(
        seat("person"),
        Err(bad(TypeError::Unknown("person".to_string())))
    );
    assert_eq!(
        seat("person(string)"),
        Err(bad(TypeError::Unknown("person".to_string())))
    );
    assert!(
        seat("person")
            .unwrap_err()
            .to_string()
            .starts_with("`person` is not a type; the types are")
    );
    assert!(matches!(
        seat("nanoid(length: \"long\")"),
        Err(EvalError::BadType(TypeError::WrongArgument { .. }))
    ));
    assert!(matches!(
        seat("nanoid(length: self)"),
        Err(EvalError::TypeMismatch { .. })
    ));
    assert_eq!(
        seat("string(prefix: \"x\")"),
        Err(bad(TypeError::UnknownArgument {
            ty: Kind::String,
            argument: "prefix".to_string()
        }))
    );
}

#[test]
fn a_type_seat_holds_a_type_and_nothing_else() {
    for written in [
        "1",
        "\"string\"",
        "1 + 1",
        "self",
        "x => x",
        "add(1, 2)",
        "var",
    ] {
        let err = seat(written).unwrap_err();
        assert!(
            err.to_string().contains("a type seat holds a type"),
            "{written}: {err}"
        );
    }
}

#[test]
fn the_type_names_are_the_kinds_and_every_one_is_answered() {
    let listed = type_names();
    let names: Vec<&str> = listed.iter().map(|(name, _)| *name).collect();
    let kinds: Vec<&str> = Kind::ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(names, kinds);
    for (name, detail) in &listed {
        assert!(!detail.is_empty(), "{name} has no description");
        assert!(
            matches!(out(name), Ok(EvalValue::Type(_))),
            "`{name}` is listed as a type and is not one"
        );
        // A type name is no reserved value, so a lambda may not be told apart from it.
        assert!(!RESERVED_VALUES.contains(name), "{name}");
        assert!(RESERVED_NAMES.iter().all(|(reserved, _)| reserved != name));
    }
}

#[test]
fn only_version_is_both_a_function_and_a_type() {
    let both: Vec<&str> = type_names()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| FUNCTIONS.iter().any(|(function, _)| function == name))
        .collect();
    assert_eq!(both, ["version"]);
}
