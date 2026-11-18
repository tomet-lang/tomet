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
fn checking_a_constructor_definition_reports_invalid_fields_and_types() {
    let valid = r#"::kind(vocabulary)
::vocabulary(catasto, version: "1.0.0")
::constructor(
  git: (
    about: | A Git repository.
    args: (
      url: (
        type: string
        positional: true
        required: true
        about: | The repository URL.
      )
      rev: (
        type: string
        default: main
        about: | A tag, branch or commit.
      )
      root: (
        type: string
        about: | The directory inside the repository that paths are relative to.
      )
    )
  )
)
"#;
    assert!(
        validate_document(&parse(valid)).is_empty(),
        "{:?}",
        validate_document(&parse(valid))
    );

    for (src, needle) in [
        (
            "::vocabulary(catasto)\n::constructor(git: (display: block))\n",
            "`git` in `::constructor`: unknown field `display`; a constructor has `about`, `args`",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: (region: body))\n",
            "`git` in `::constructor`: unknown field `region`; a constructor has `about`, `args`",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: (singleton: true))\n",
            "`git` in `::constructor`: unknown field `singleton`; a constructor has `about`, `args`",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: (content: (allow: any)))\n",
            "`git` in `::constructor`: unknown field `content`; a constructor has `about`, `args`",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: (args: (url: (kind: string))))\n",
            "`git.url` in `::constructor`: unknown field `kind`; an argument has `type`, `positional`, `required`, `default`, `about`",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: (args: (url: (type: person))))\n",
            "`git.url` in `::constructor`: `person` is not a type",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: (args: (url: (positional: yes))))\n",
            "`git.url` in `::constructor`: `positional`: the text \"yes\" is not `true` or `false`",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: (args: (url: (required: 1))))\n",
            "`git.url` in `::constructor`: `required`: the integer `1` is not `true` or `false`",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: (args: (n: (type: uint, default: \"abc\"))))\n",
            "`git.n` in `::constructor`: `default` is not of `type: uint`: the text \"abc\" is not a non-negative integer",
        ),
        (
            "::vocabulary(catasto)\n::constructor(git: ())\n::constructor(git: ())\n",
            "`git` in `::constructor`: duplicate constructor `git`",
        ),
    ] {
        let errors: Vec<String> = validate_document(&parse(src))
            .iter()
            .map(|e| e.to_string())
            .collect();
        assert_eq!(errors.len(), 1, "{src}: {errors:?}");
        assert!(errors[0].contains(needle), "{src}: got {:?}", errors[0]);
    }
}

const CATASTO_VOCAB_FOR_CALLS: &str = r#"::kind(vocabulary)
::vocabulary(catasto, version: "1.0.0")
::constructor(
  git: (
    about: | A Git repository.
    args: (
      url: (
        type: string
        positional: true
        required: true
        about: | The repository URL.
      )
      rev: (
        type: string
        default: main
        about: | A tag, branch or commit.
      )
      root: (
        type: string
        about: | The directory inside the repository that paths are relative to.
      )
      depth: (
        type: uint
        about: | Optional shallow clone depth.
      )
    )
  )
  upstream: (
    about: | A file of a named upstream.
    args: (
      name: (
        type: string
        positional: true
        required: true
        about: | The name given in `::upstreams`.
      )
      path: (
        type: string
        required: true
        about: | The file inside the upstream.
      )
    )
  )
)
"#;

fn catasto_vocab() -> tomet_semantics::Vocabulary {
    tomet_semantics::Vocabulary::from_file(&parse(CATASTO_VOCAB_FOR_CALLS)).expect("vocabulary")
}

#[test]
fn constructor_calls_in_tmt_and_ctmt_are_checked_against_the_vocabulary() {
    let vocab = catasto_vocab();

    // 1. Valid calls in a .tmt document (with ::use(catasto)), plus bare list(string) and string("x")
    let tmt_ok = r#"::use(catasto, version: "1.0.0")
::var(tags: (type: list(string), default: (string("x"))))
::meta(
  repo: catasto.git("https://github.com/tomet-lang/commonfiles.git", rev: main, root: base)
  file: catasto.upstream(commonfiles, path: ".editorconfig")
)
"#;
    let doc = parse(tmt_ok);
    let bindings = Bindings::for_document(&doc, [vocab.clone()]);
    let errors = validate_document_with(&doc, &bindings);
    assert!(errors.is_empty(), "{errors:?}");

    // 2. Without ::use(catasto), namespace is not in scope, reported at the span of the call
    let no_use = "::meta(repo: catasto.git(\"https://example.com\"))\n";
    let doc_no_use = parse(no_use);
    let bindings_no_use = Bindings::for_document(&doc_no_use, [vocab.clone()]);
    let errors = validate_document_with(&doc_no_use, &bindings_no_use);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].to_string(),
        "`catasto.git`: the namespace `catasto` is not in scope -- declare its vocabulary in `vocabularies` and write `::use(catasto)`"
    );
    assert_eq!(
        &no_use[errors[0].span().start..errors[0].span().end],
        "catasto.git(\"https://example.com\")"
    );

    // 3. Each constructor call error in .tmt
    for (call_expr, expected) in [
        (
            "catasto.svn(\"https://example.com\")",
            "`catasto.svn`: the namespace `catasto` declares no constructor `svn`; its constructors are `git`, `upstream`",
        ),
        (
            "catasto.git(\"https://example.com\", branch: main)",
            "`catasto.git` has no argument `branch`; its vocabulary declares the ones it takes under `args:`",
        ),
        (
            "catasto.git(rev: main)",
            "`catasto.git` requires the argument `url`",
        ),
        (
            "catasto.git(\"https://example.com\", depth: -1)",
            "`catasto.git`'s argument `depth` is declared `type: uint`, but the integer `-1` is not a non-negative integer",
        ),
        (
            "catasto.git(\"https://example.com\", \"main\")",
            "`catasto.git` takes at most 1 positional argument (`url`), found 2",
        ),
        (
            "catasto.git(\"https://example.com\", url: \"https://other.com\")",
            "`catasto.git`: the argument `url` is written both by position and by name",
        ),
    ] {
        let src = format!("::use(catasto)\n::meta(x: {call_expr})\n");
        let doc = parse(&src);
        let bindings = Bindings::for_document(&doc, [vocab.clone()]);
        let errors = validate_document_with(&doc, &bindings);
        assert_eq!(errors.len(), 1, "{call_expr}: {errors:?}");
        assert_eq!(errors[0].to_string(), expected, "{call_expr}");
        assert_eq!(
            &src[errors[0].span().start..errors[0].span().end],
            call_expr,
            "span must match the call"
        );
    }

    // 4. Constructor calls in a .ctmt file (header and body)
    let ctmt_src = "::use(catasto, version: \"1.0.0\")\n::upstreams(\n  commonfiles: catasto.git(\"https://github.com/tomet-lang/commonfiles.git\", rev: main, root: base)\n)\n\".editorconfig\": (source: catasto.upstream(commonfiles, path: \".editorconfig\"))\n";
    let ctmt_file = tomet_ast_parser::parse_data_file(ctmt_src).expect("parses");
    let ctmt_bindings = Bindings::for_document(&ctmt_file, [vocab.clone()]);
    let mut ctmt_errors = Vec::new();
    check_ctmt_calls(&ctmt_file, &ctmt_bindings, &mut ctmt_errors);
    assert!(ctmt_errors.is_empty(), "{ctmt_errors:?}");

    // And a bad call in the .ctmt body has its span shifted past the header
    let bad_ctmt_src =
        "::use(catasto)\n\".editorconfig\": (source: catasto.upstream(commonfiles))\n";
    let bad_ctmt = tomet_ast_parser::parse_data_file(bad_ctmt_src).expect("parses");
    let bad_bindings = Bindings::for_document(&bad_ctmt, [vocab]);
    let mut bad_errors = Vec::new();
    check_ctmt_calls(&bad_ctmt, &bad_bindings, &mut bad_errors);
    assert_eq!(bad_errors.len(), 1, "{bad_errors:?}");
    assert_eq!(
        bad_errors[0].to_string(),
        "`catasto.upstream` requires the argument `path`"
    );
    assert_eq!(
        &bad_ctmt_src[bad_errors[0].span().start..bad_errors[0].span().end],
        "catasto.upstream(commonfiles)"
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

    // 2. Non-standard declarations and ::use are accepted; NotForThisHost, Repeated, and InvalidDefinition are reported
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
        Declaration::new(
            "export",
            Some(Tuple::from_named([(
                "type".into(),
                Value::Scalar(Scalar::Text("commonmark".into())),
            )])),
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
        Diagnostic::NotForThisHost { declaration, .. } if declaration == "export"
    )));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::DuplicateSingleton { name, first, duplicate } if name == "meta" && *first == spans[4] && *duplicate == spans[5])
    ));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::InvalidDefinition { declaration: "kind", name, reason, .. } if name == "version" && reason == "the text \"abc\" is not a version such as `1.10.0`")
    ));
    assert!(errors.iter().any(
        |e| matches!(e, Diagnostic::InvalidDefinition { declaration: "macro", name, .. } if name == "bad")
    ));
}
