//! Tests for `$cache.<kind>(...)` expression evaluation.

use super::*;

fn eval_with_resolver(src: &str, resolver: &dyn Resolver) -> Result<EvalValue, EvalError> {
    let var = EvalValue::Scalar(Scalar::None);
    let macros = HashMap::new();
    let cx = EvalContext {
        resolver,
        self_doc: DocRef("here.tmt".to_string()),
        macros: &macros,
        var: &var,
        var_unset: &[],
        includes: &[],
        heading_level: 0,
        notes: Default::default(),
    };
    evaluate(&parse(src), &cx)
}

#[test]
fn cache_returns_scalar_text() {
    let resolver = MemoryResolver::new().with_cache(
        "command",
        "git rev-parse --short HEAD",
        text("a1b2c3d"),
    );
    assert_eq!(
        eval_with_resolver(r#"cache.command("git rev-parse --short HEAD")"#, &resolver),
        Ok(text("a1b2c3d"))
    );
    assert_eq!(
        resolver.cache_requests(),
        vec![CacheRequest {
            kind: "command".into(),
            key: "git rev-parse --short HEAD".into(),
            local: false,
            inputs: vec![],
        }]
    );
}

#[test]
fn cache_composes_with_raw_fragment() {
    let resolver = MemoryResolver::new().with_cache(
        "command",
        "tomet --help",
        text("Usage: tomet [COMMAND]\n"),
    );
    let val = eval_with_resolver(
        r#"raw(cache.command("tomet --help"), lang: "text")"#,
        &resolver,
    )
    .unwrap();
    let expected = eval_with_resolver(r#"raw("Usage: tomet [COMMAND]\n", lang: "text")"#, &resolver)
        .unwrap();
    assert_eq!(val, expected);
}

#[test]
fn cache_returns_structured_map_and_tuple_for_field_access() {
    let resolver = MemoryResolver::new().with_cache(
        "command",
        "status",
        map(vec![
            ("stdout", text("ok")),
            ("code", int(0)),
            ("items", seq(vec![text("first"), text("second")])),
        ]),
    );
    assert_eq!(
        eval_with_resolver(r#"cache.command("status").stdout"#, &resolver),
        Ok(text("ok"))
    );
    assert_eq!(
        eval_with_resolver(r#"cache.command("status").code"#, &resolver),
        Ok(int(0))
    );
    assert_eq!(
        eval_with_resolver(r#"cache.command("status").items.item(1)"#, &resolver),
        Ok(text("second"))
    );
}

#[test]
fn cache_passes_local_flag_and_inputs_tuple() {
    let resolver = MemoryResolver::new()
        .with_local_cache("command", "whoami", text("tefla"))
        .with_cache("command", "wc -l < scripts/deploy.sh", text("42"))
        .with_cache("ai", "summarize", text("done"));

    assert_eq!(
        eval_with_resolver(r#"cache.command("whoami", local: true)"#, &resolver),
        Ok(text("tefla"))
    );
    assert!(matches!(
        eval_with_resolver(r#"cache.command("whoami")"#, &resolver),
        Err(EvalError::Unresolved(_))
    ));

    // Single-item `("scripts/deploy.sh")` parses as Group in expressions and is accepted.
    assert_eq!(
        eval_with_resolver(
            r#"cache.command("wc -l < scripts/deploy.sh", inputs: ("scripts/deploy.sh"))"#,
            &resolver
        ),
        Ok(text("42"))
    );
    assert_eq!(
        eval_with_resolver(
            r#"cache.ai("summarize", inputs: ("a.tmt", "b.tmt"), local: false)"#,
            &resolver
        ),
        Ok(text("done"))
    );

    assert_eq!(
        resolver.cache_requests(),
        vec![
            CacheRequest {
                kind: "command".into(),
                key: "whoami".into(),
                local: true,
                inputs: vec![],
            },
            CacheRequest {
                kind: "command".into(),
                key: "whoami".into(),
                local: false,
                inputs: vec![],
            },
            CacheRequest {
                kind: "command".into(),
                key: "wc -l < scripts/deploy.sh".into(),
                local: false,
                inputs: vec!["scripts/deploy.sh".into()],
            },
            CacheRequest {
                kind: "ai".into(),
                key: "summarize".into(),
                local: false,
                inputs: vec!["a.tmt".into(), "b.tmt".into()],
            },
        ]
    );
}

#[test]
fn cache_rejects_uncalled_member_and_bad_arguments() {
    let resolver = MemoryResolver::new();

    // Missing call parens
    assert!(eval_with_resolver("cache.command", &resolver).is_err());

    // Wrong positional arity or type
    assert!(matches!(
        eval_with_resolver("cache.command()", &resolver),
        Err(EvalError::WrongArgCount { .. })
    ));
    assert!(matches!(
        eval_with_resolver(r#"cache.command("a", "b")"#, &resolver),
        Err(EvalError::WrongArgCount { .. })
    ));
    assert!(matches!(
        eval_with_resolver("cache.command(42)", &resolver),
        Err(EvalError::TypeMismatch { .. })
    ));

    // Wrong named argument types or unknown/duplicate named arguments
    assert!(matches!(
        eval_with_resolver(r#"cache.command("k", local: "yes")"#, &resolver),
        Err(EvalError::TypeMismatch { .. })
    ));
    assert!(matches!(
        eval_with_resolver(r#"cache.command("k", inputs: "a.txt")"#, &resolver),
        Err(EvalError::TypeMismatch { .. })
    ));
    assert!(matches!(
        eval_with_resolver(r#"cache.command("k", inputs: (1, 2))"#, &resolver),
        Err(EvalError::TypeMismatch { .. })
    ));
    assert!(matches!(
        eval_with_resolver(r#"cache.command("k", inputs: (1))"#, &resolver),
        Err(EvalError::TypeMismatch { .. })
    ));
    assert!(eval_with_resolver(r#"cache.command("k", inputs: (a: "x"))"#, &resolver).is_err());
    assert!(eval_with_resolver(r#"cache.command("k", unknown: 1)"#, &resolver).is_err());
    assert!(eval_with_resolver(r#"cache.command("k", local: true, local: false)"#, &resolver).is_err());
    assert!(eval_with_resolver(r#"cache.command("k", inputs: (), inputs: ())"#, &resolver).is_err());

    // Missing cache entry / NoResolver
    assert!(matches!(
        eval_with_resolver(r#"cache.command("missing")"#, &resolver),
        Err(EvalError::Unresolved(_))
    ));
    assert!(matches!(
        out(r#"cache.command("missing")"#),
        Err(EvalError::Unresolved(_))
    ));
}
