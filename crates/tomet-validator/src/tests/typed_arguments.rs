//! An argument held to the type its parameter declares.

use super::*;

/// An argument is held to the type its parameter declares. A bare scalar of a text
/// type is the text it was written as, so `2024` is a `string` and `1.10` a `version`.
#[test]
fn an_argument_is_held_to_the_type_its_parameter_declares() {
    for ok in [
        "@entry(3)",
        "@entry(id: 0, label: 2024, rev: 1.10, done: true)",
        "@entry(label: hello, rev: \"1.10.0\")",
        "@entry(rev: 2)",
        "@entry(label: true)",
        // not known until evaluated
        "@entry(id: $(vars.n))",
        // `type: person` names no type: not checked
        "@entry(owner: anyone, tags: (a, b))",
    ] {
        assert!(
            argument_problems(ok).is_empty(),
            "{ok}: {:?}",
            argument_problems(ok)
        );
    }
    for (bad, needle) in [
        (
            "@entry(id: abc)",
            "the text \"abc\" is not a non-negative integer",
        ),
        (
            "@entry(id: -1)",
            "the integer `-1` is not a non-negative integer",
        ),
        (
            "@entry(id: \"3\")",
            "the text \"3\" is not a non-negative integer",
        ),
        (
            "@entry(rev: 1.0.0_beta)",
            "the text \"1.0.0_beta\" is not a version",
        ),
        (
            "@entry(done: yes)",
            "the text \"yes\" is not `true` or `false`",
        ),
        ("@entry(label: (a))", "a list is not a string"),
        ("@entry(tags: 5)", "is not a list"),
    ] {
        let found = argument_problems(bad);
        assert_eq!(found.len(), 1, "{bad}: {found:?}");
        assert!(found[0].contains(needle), "{bad}: {found:?}");
        assert!(found[0].contains("is declared `type:"), "{bad}: {found:?}");
    }
}

/// Checking the vocabulary document itself reports the parameters it declares in a
/// way that cannot be held to.
#[test]
fn checking_a_vocabulary_reports_a_type_that_names_no_type() {
    let errors = validate_document(&parse(TYPED_DECK));
    let problems: Vec<String> = errors
        .iter()
        .filter(|e| matches!(e, Diagnostic::InvalidDefinition { declaration, .. } if *declaration == "element"))
        .map(|e| e.to_string())
        .collect();
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems
            .iter()
            .any(|p| p.contains("`entry.owner`") && p.contains("`person` is not a type")),
        "{problems:?}"
    );
    assert!(
        problems
            .iter()
            .any(|p| p.contains("`entry.unsupported`") && p.contains("`set` is not a type")),
        "{problems:?}"
    );
}

#[test]
fn check_ctmt_declarations_reports_expected_diagnostics() {
    use tomet_ast::Tuple;
    use tomet_ast::Value;
    use tomet_declaration::Declaration;
    use tomet_span::Span;

    // 1. Valid declarations produce no errors
    let declared = vec![
        Declared::new(
            Declaration::new(
                "kind",
                Some(Tuple::from_unnamed([
                    Value::Scalar(Scalar::Text("catalog".into())),
                    Value::Scalar(Scalar::Int(1)),
                ])),
                None,
            ),
            Span::new(0, 10),
        ),
        Declared::new(
            Declaration::new(
                "version",
                Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
                    "1.0.0".into(),
                ))])),
                None,
            ),
            Span::new(11, 25),
        ),
        Declared::new(
            Declaration::new(
                "meta",
                Some(Tuple::from_named([(
                    "author".into(),
                    Value::Scalar(Scalar::Text("me".into())),
                )])),
                None,
            ),
            Span::new(26, 40),
        ),
    ];
    let mut errors = Vec::new();
    check_ctmt_declarations(&declared, &mut errors);
    assert_eq!(errors, vec![]);

    // 2. Non-standard declarations are passed over; NotForThisHost, Repeated, InvalidVersion, and InvalidDefinition are reported
    let decls = vec![
        Declaration::new("upstreams", None, None),
        Declaration::new("foo.bar", None, None),
        Declaration::new(
            "use",
            Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
                "deck".into(),
            ))])),
            None,
        ),
        Declaration::new("meta", None, None),
        Declaration::new("meta", None, None),
        Declaration::new(
            "kind",
            Some(Tuple::from_unnamed([
                Value::Scalar(Scalar::Text("catalog".into())),
                Value::Scalar(Scalar::Text("abc".into())),
            ])),
            None,
        ),
        Declaration::new(
            "macro",
            Some(Tuple::from_named([(
                "bad".into(),
                Value::Scalar(Scalar::Int(123)),
            )])),
            None,
        ),
    ];
    let spans: Vec<Span> = (0..decls.len())
        .map(|i| Span::new(i * 10, i * 10 + 9))
        .collect();
    let declared: Vec<_> = decls
        .into_iter()
        .zip(spans.iter().copied())
        .map(|(d, s)| Declared::new(d, s))
        .collect();
    let mut errors = Vec::new();
    check_ctmt_declarations(&declared, &mut errors);

    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownElement { .. }))
    );
    assert_eq!(errors.len(), 4);
    assert!(errors.iter().any(|e| matches!(
        e,
        Diagnostic::NotForThisHost {
            declaration: "use",
            ..
        }
    )));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::DuplicateSingleton { name, first, duplicate } if name == "meta" && *first == spans[3] && *duplicate == spans[4])
    ));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::InvalidVersion { element, found, .. } if element == "kind" && found == "the text \"abc\"")
    ));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::InvalidDefinition { declaration: "macro", name, .. } if name == "bad")
    ));
}
