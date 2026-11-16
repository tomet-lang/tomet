//! A version as a declared or a written value.

use super::*;

/// The version slot of `::kind` and `::use` takes a `version`, the same type `::version` takes.
#[test]
fn kind_use_and_version_accept_and_reject_the_same_values() {
    let accepted = [
        "1",
        "0",
        "1.0",
        "1.10",
        "1.0.0",
        "\"1.0.0\"",
        "1.0.0-beta",
        "1.0.0+5",
    ];
    assert!(version_problems("::kind(note)\n::use(ns)\n").is_empty());
    for ver in accepted {
        for src in [
            format!("::kind(note, {ver})\n"),
            format!("::kind(note, version: {ver})\n"),
            format!("::kind(note)\n::use(ns, {ver})\n"),
            format!("::kind(note)\n::use(ns, version: {ver})\n"),
            format!("::version({ver})\n"),
        ] {
            assert!(
                version_problems(&src).is_empty(),
                "{src}: {:?}",
                version_problems(&src)
            );
        }
    }

    let rejected = [
        ("-1", "the integer `-1` is not a version such as `1.10.0`"),
        ("abc", "the text \"abc\" is not a version such as `1.10.0`"),
        ("\"1.\"", "the text \"1.\" is not a version such as `1.10.0`"),
        ("\".1\"", "the text \".1\" is not a version such as `1.10.0`"),
        (
            "\"1..2\"",
            "the text \"1..2\" is not a version such as `1.10.0`",
        ),
        ("\"\"", "the text \"\" is not a version such as `1.10.0`"),
        ("true", "`true` is not a version such as `1.10.0`"),
        ("(1, 2)", "a list is not a version such as `1.10.0`"),
        ("none", "`none` is not a version such as `1.10.0`"),
    ];
    for (ver, reason) in rejected {
        for (decl, src) in [
            ("kind", format!("::kind(note, {ver})\n")),
            ("kind", format!("::kind(note, version: {ver})\n")),
            ("use", format!("::kind(note)\n::use(ns, {ver})\n")),
            ("use", format!("::kind(note)\n::use(ns, version: {ver})\n")),
            ("version", format!("::version({ver})\n")),
        ] {
            assert_eq!(
                version_problems(&src),
                [format!("`version` in `::{decl}`: {reason}")],
                "{src}"
            );
        }
    }
}

#[test]
fn kind_and_use_take_a_name_and_a_version_and_nothing_more() {
    for (src, element) in [
        ("::kind(note, 1, 2)\n", "kind"),
        ("::kind(note)\n::use(deck, 1, 2)\n", "use"),
    ] {
        let errors = validate_document(&parse(src));
        assert!(
            errors.iter().any(|e| matches!(
                e,
                Diagnostic::TooManyArguments { element: el, takes: 2, .. } if el == element
            )),
            "{src:?}: {errors:?}"
        );
    }
}

#[test]
fn the_version_messages_say_what_to_write() {
    assert_eq!(
        version_problems("::kind(note, abc)\n"),
        ["`version` in `::kind`: the text \"abc\" is not a version such as `1.10.0`"]
    );

    let errors = validate_document(&parse("::kind(note, 1, 2)\n"));
    let message = errors
        .iter()
        .find(|e| matches!(e, Diagnostic::TooManyArguments { .. }))
        .expect("an extra argument is reported")
        .to_string();
    assert_eq!(
        message,
        "`kind` takes at most 2 arguments: the name, then an optional version"
    );
}

/// A document's declared version is a version: `1.10` is the version `1.10`, bare or
/// quoted, and `abc` or `1.0.0_beta` is not one.
#[test]
fn a_declared_version_is_held_to_the_version_type() {
    for ok in [
        "::vocabulary(deck, version: \"1.0.0\")\n",
        "::vocabulary(deck, version: \"1.0-beta\")\n",
        "::vocabulary(deck, version: 1.10)\n",
        "::vocabulary(deck, version: 2)\n",
        "::blueprint(note){ version: \"1.0\", description: \"x\" }\n",
        "::version(1.0)\n",
        "::version(1.10)\n",
        "::version(1.10.0)\n",
    ] {
        assert!(
            version_problems(ok).is_empty(),
            "{ok}: {:?}",
            version_problems(ok)
        );
    }
    for (bad, needle) in [
        (
            "::vocabulary(deck, version: \"abc\")\n",
            "the text \"abc\" is not a version",
        ),
        (
            "::vocabulary(deck, version: \"1.0.0_beta\")\n",
            "the text \"1.0.0_beta\"",
        ),
        (
            "::blueprint(note){ version: true }\n",
            "`true` is not a version",
        ),
        ("::version(v1)\n", "the text \"v1\" is not a version"),
    ] {
        let found = version_problems(bad);
        assert_eq!(found.len(), 1, "{bad}: {found:?}");
        assert!(found[0].contains(needle), "{bad}: {found:?}");
    }
}

/// A `::meta` field declared `type: version` holds a bare `1.10` as the text it was
/// written as.
#[test]
fn a_version_field_in_meta_reads_a_bare_number_as_text() {
    use std::collections::BTreeMap;
    let types: BTreeMap<String, String> = [("rev".to_string(), "version".to_string())]
        .into_iter()
        .collect();
    let problems = |src: &str| {
        let mut errors = Vec::new();
        check_field_types(&parse(src), &types, &mut errors);
        errors.len()
    };
    for ok in ["1.10", "\"1.10.0\"", "\"1.0-beta\"", "2", "1.0"] {
        assert_eq!(problems(&format!("::meta(rev: {ok})\n")), 0, "{ok}");
    }
    for bad in ["\"abc\"", "true", "\"1.0.0_beta\""] {
        assert_eq!(problems(&format!("::meta(rev: {bad})\n")), 1, "{bad}");
    }
}
