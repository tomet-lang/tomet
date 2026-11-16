//! Type seats: where one may stand, and what it is held to.

use super::*;

/// The type seat diagnostics of `src`.
fn seat_diagnostics(src: &str) -> Vec<Diagnostic> {
    validate_document(&parse(src))
        .into_iter()
        .filter(|d| {
            matches!(
                d,
                Diagnostic::TypeSeatOutsideBlueprint { .. } | Diagnostic::InvalidTypeSeat { .. }
            )
        })
        .collect()
}

/// A seat is the blueprint's: written in any other document it has nothing to be filled by,
/// wherever it sits.
#[test]
fn a_type_seat_outside_a_blueprint_is_an_error_wherever_it_sits() {
    for (src, seat) in [
        (
            "::meta(id: <$uuid(default: auto)>)\n",
            "<$uuid(default: auto)>",
        ),
        ("::kind(note)\n\n<$string>\n", "<$string>"),
        (
            "::kind(note)\n\nWritten on <$date(default: auto)>.\n",
            "<$date(default: auto)>",
        ),
        ("::kind(note)\n\n=[ Notes of <$date> ]\n", "<$date>"),
        ("@entry(count: <$int(default: 3)>)\n", "<$int(default: 3)>"),
    ] {
        let found = seat_diagnostics(src);
        assert_eq!(found.len(), 1, "{src}: {found:?}");
        assert!(
            matches!(&found[0], Diagnostic::TypeSeatOutsideBlueprint { seat: s, .. } if s == seat),
            "{src}: {found:?}"
        );
        assert_eq!(found[0].severity(), Severity::Error);
        let at = found[0].span().start;
        assert!(src[at..].starts_with(seat), "{src}: {:?}", found[0].span());
    }
}

#[test]
fn a_type_seat_outside_a_blueprint_says_where_it_belongs() {
    let found = seat_diagnostics("::kind(note)\n\n<$string>\n");
    assert_eq!(
        found[0].to_string(),
        "`<$string>` is a type seat, and type seats belong in a blueprint (`::blueprint(...)`): \
         a document made from one has none"
    );
}

/// Both ways a document says it is a blueprint hold its seats.
#[test]
fn a_blueprint_may_hold_type_seats() {
    for src in [
        "::kind(blueprint)\n::blueprint(note)\n::id(<$uuid(default: auto)>)\n",
        "::blueprint(note)\n::meta(created: <$datetime(default: auto)>)\n",
        "::kind(blueprint)\n::meta(day: <$date(default: auto)>)\n",
        "::blueprint(note)\n\n=[ Notes of <$date(default: auto)> ]\n\n<$string(default: \"x\")>\n",
        // No `default` is no problem of the seat's type: whether there is a value for it is for
        // `tomet new` to say.
        "::blueprint(note)\n::meta(title: <$string>)\n",
        "::blueprint(note)\n::meta(count: <$int(default: 3)>, tags: <$list(string, length: 2)>)\n",
        "::blueprint(note)\n::id(<$nanoid(length: 8, prefix: \"note-\", default: auto)>)\n",
    ] {
        assert_eq!(validate_document(&parse(src)), vec![], "{src}");
    }
}

/// What is written as the seat's type has to be a type, and the error is the type crate's own.
#[test]
fn a_type_seat_that_is_not_a_type_is_an_error_in_the_type_crates_words() {
    for (seat, needle) in [
        ("<$person>", "`person` is not a type; the types are"),
        ("<$person(string)>", "`person` is not a type; the types are"),
        (
            "<$string(prefix: \"x\")>",
            "`string` has no argument `prefix`; it takes `default`",
        ),
        ("<$int(default: auto)>", "`int` has no `default: auto`"),
        (
            "<$list(string, string)>",
            "`list` takes 1 argument written by position",
        ),
        ("<$nanoid(length: 0)>", "`nanoid`: `length` takes"),
        (
            "<$add(1, 2)>",
            "a type seat holds a type, and `add(1, 2)` is",
        ),
    ] {
        let src = format!("::blueprint(note)\n::meta(x: {seat})\n");
        let found = seat_diagnostics(&src);
        assert_eq!(found.len(), 1, "{seat}: {found:?}");
        let Diagnostic::InvalidTypeSeat {
            seat: written,
            reason,
            span,
        } = &found[0]
        else {
            panic!("{seat}: {found:?}");
        };
        assert_eq!(written, seat);
        assert!(reason.contains(needle), "{seat}: {reason}");
        assert!(src[span.start..].starts_with(seat), "{seat}: {span:?}");
        assert_eq!(
            found[0].to_string(),
            format!("type seat `{seat}`: {reason}")
        );
    }
}

/// The seat's own `default`, when it is a value written out, is a value of the seat's type.
#[test]
fn a_type_seat_default_that_is_not_of_its_type_is_an_error() {
    for (seat, needle) in [
        (
            "<$int(default: \"three\")>",
            "`default`: the text \"three\" is not an integer",
        ),
        (
            "<$date(default: \"soon\")>",
            "`default`: the text \"soon\" is not a date",
        ),
        (
            "<$bool(default: \"yes\")>",
            "`default`: the text \"yes\" is not `true` or `false`",
        ),
        (
            "<$list(string, default: 3)>",
            "`default`: the integer `3` is not a list of a string",
        ),
        (
            "<$uuid(default: \"not-a-uuid\")>",
            "`default`: the text \"not-a-uuid\" is not a UUID",
        ),
    ] {
        let found = seat_diagnostics(&format!("::blueprint(note)\n::meta(x: {seat})\n"));
        assert_eq!(found.len(), 1, "{seat}: {found:?}");
        assert!(
            matches!(&found[0], Diagnostic::InvalidTypeSeat { reason, .. } if reason.contains(needle)),
            "{seat}: {found:?}"
        );
    }
    // And a default that fits, or is `auto`, is not.
    for seat in [
        "<$int(default: 3)>",
        "<$bool(default: true)>",
        "<$date(default: \"2026-11-11\")>",
        "<$date(default: auto)>",
    ] {
        let found = seat_diagnostics(&format!("::blueprint(note)\n::meta(x: {seat})\n"));
        assert_eq!(found, vec![], "{seat}");
    }
}

/// Every seat is reported, not the first.
#[test]
fn every_wrong_type_seat_of_a_blueprint_is_reported() {
    let src = "::blueprint(note)\n::meta(a: <$person>, b: <$int>, c: <$int(default: auto)>)\n\n=[ <$date(default: 3)> ]\n";
    let found = seat_diagnostics(src);
    assert_eq!(found.len(), 3, "{found:?}");
}

/// A seat that takes something from outside the text it is written in (a variable, the vault) is
/// judged when it is filled, where those are known: not here, where it cannot be told.
#[test]
fn a_type_seat_that_reads_from_outside_is_judged_when_it_is_filled() {
    for seat in [
        "<$list(string, length: var.n)>",
        "<$person(var.kind)>",
        "<$string(default: macro.x())>",
    ] {
        let found = seat_diagnostics(&format!("::blueprint(note)\n::meta(x: {seat})\n"));
        assert_eq!(found, vec![], "{seat}");
    }
}

/// A seat is not known until it is filled, as a `{$ ... }` is not until it is evaluated: a field
/// the settings give a `type:` is not reported for holding one.
#[test]
fn a_type_seat_in_a_typed_field_is_unknown_until_it_is_filled() {
    for (src, declared) in [
        (
            "::blueprint(note)\n::meta(created: <$datetime(default: auto)>)\n",
            ("created", "datetime"),
        ),
        (
            "::blueprint(note)\n::meta(day: <$date(default: auto)>)\n",
            ("day", "date"),
        ),
        (
            "::blueprint(note)\n::id(<$uuid(default: auto)>)\n",
            ("id", "uuid"),
        ),
        (
            "::blueprint(note)\n::meta(rev: <$version(default: \"1.0\")>)\n",
            ("rev", "version"),
        ),
    ] {
        assert_eq!(field_diagnostics(src, &[declared]), vec![], "{src}");
    }
    // A value that is written is still held to the type.
    let errors = field_diagnostics(
        "::blueprint(note)\n::meta(created: \"soon\")\n",
        &[("created", "datetime")],
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

/// The same for an argument held to the `type:` its parameter declares, and for a declared version.
#[test]
fn a_type_seat_as_an_argument_or_a_version_is_unknown_until_it_is_filled() {
    for ok in [
        "@entry(id: <$uint(default: 1)>)",
        "@entry(rev: <$version(default: \"1.0\")>, done: <$bool(default: true)>)",
    ] {
        assert!(
            argument_problems(ok).is_empty(),
            "{ok}: {:?}",
            argument_problems(ok)
        );
    }
    assert_eq!(
        version_problems("::blueprint(note){ version: <$version(default: \"1.0\")> }\n"),
        Vec::<String>::new()
    );
}
