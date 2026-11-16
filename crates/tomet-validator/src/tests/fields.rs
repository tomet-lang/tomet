//! A field held to its type: uuid, date and datetime.

use super::*;

#[test]
fn a_uuid_field_holding_a_uuid_is_clean() {
    let source = format!("::id({UUID})\n");
    assert_eq!(field_diagnostics(&source, &[("id", "uuid")]), vec![]);
    let upper = format!("::id({})\n", UUID.to_uppercase());
    assert_eq!(field_diagnostics(&upper, &[("id", "uuid")]), vec![]);
}

#[test]
fn a_uuid_field_holding_anything_else_is_reported() {
    for bad in [
        "not-a-uuid",
        "123e4567e89b42d3a456426614174000",
        "123e4567-e89b-42d3-a456-42661417400",
        "123e4567-e89b-42d3-a456-42661417400g",
    ] {
        let source = format!("::id(\"{bad}\")\n");
        let errors = field_diagnostics(&source, &[("id", "uuid")]);
        assert_eq!(errors.len(), 1, "{bad}: {errors:?}");
        assert!(
            matches!(&errors[0], Diagnostic::InvalidFieldValue { field, value, .. }
                if field == "id" && value == bad),
            "{errors:?}"
        );
        assert_eq!(errors[0].severity(), Severity::Error);
    }
    let number = field_diagnostics("::id(42)\n", &[("id", "uuid")]);
    assert_eq!(number.len(), 1, "{number:?}");
}

/// A `date` field is a calendar day and nothing after it; a `datetime` field takes the same day
/// with a time too.
#[test]
fn a_date_field_holds_a_calendar_day_and_a_datetime_field_holds_more() {
    let date = [("day", "date")];
    assert_eq!(
        field_diagnostics("::meta(day: 2026-10-25)\n", &date),
        vec![]
    );
    assert_eq!(
        field_diagnostics("::meta(day: \"2024-02-29\")\n", &date),
        vec![]
    );
    for bad in ["2026-02-29", "2026-10-25T09:00:00Z", "tomorrow"] {
        let errors = field_diagnostics(&format!("::meta(day: \"{bad}\")\n"), &date);
        assert_eq!(errors.len(), 1, "{bad}: {errors:?}");
        assert!(
            matches!(&errors[0], Diagnostic::InvalidFieldValue { field, field_type, value, .. }
                if field == "day" && field_type == "date" && value == bad),
            "{errors:?}"
        );
    }
    let datetime = [("day", "datetime")];
    assert_eq!(
        field_diagnostics("::meta(day: \"2026-10-25T09:00:00Z\")\n", &datetime),
        vec![]
    );
}

/// What is not there to be wrong is not reported: a field the document
/// leaves out, one left empty for `tomet format` to generate, and an
/// expression that has not been evaluated yet (a blueprint's
/// `$(var.id)`).
#[test]
fn an_absent_empty_or_unevaluated_uuid_is_not_reported() {
    let declared = [("id", "uuid")];
    assert_eq!(field_diagnostics("::meta(title: x)\n", &declared), vec![]);
    assert_eq!(field_diagnostics("::id(\"\")\n", &declared), vec![]);
    assert_eq!(field_diagnostics("::id($(var.id))\n", &declared), vec![]);
    assert_eq!(field_diagnostics("plain\n", &declared), vec![]);
}

/// Only the fields the settings declare are looked at, and only the types
/// that have a form.
#[test]
fn an_undeclared_field_or_a_type_with_no_form_is_left_alone() {
    let source = "::id(nonsense)\n::meta(title: nonsense)\n";
    assert_eq!(field_diagnostics(source, &[]), vec![]);
    assert_eq!(field_diagnostics(source, &[("title", "string")]), vec![]);
    assert_eq!(field_diagnostics(source, &[("id", "uuid")]).len(), 1);
}

#[test]
fn a_datetime_field_has_to_read_as_one() {
    let declared = [("created", "datetime")];
    for good in [
        "2026-10-17",
        "2026-10-17T09:00:00+09:00",
        "2026-10-17T09:00",
        "2026-10-17 09:00:00",
    ] {
        let source = format!("::meta(created: {good})\n");
        assert_eq!(field_diagnostics(&source, &declared), vec![], "{good}");
    }
    for bad in ["yesterday", "2026-13-01", "2026-10-17T25:00", "2026-1-1"] {
        let source = format!("::meta(created: \"{bad}\")\n");
        let errors = field_diagnostics(&source, &declared);
        assert_eq!(errors.len(), 1, "{bad}: {errors:?}");
        assert!(
            errors[0].to_string().contains("not a valid datetime"),
            "{}",
            errors[0]
        );
    }
    // Absent, empty and not yet evaluated are not reported, as for `uuid`.
    assert_eq!(field_diagnostics("::meta(title: x)\n", &declared), vec![]);
    assert_eq!(
        field_diagnostics("::meta(created: $(var.when))\n", &declared),
        vec![]
    );
}

/// A document may write a version after the name in `::kind` and `::use`, or leave it out.
#[test]
fn a_version_after_the_name_of_kind_and_use_is_fine_when_it_is_a_version() {
    for src in [
        "::kind(note)\n::use(deck)\n",
        "::kind(note, 1)\n::use(deck, 3)\n",
        "::kind(note, version: 2)\n",
    ] {
        let errors = validate_document(&parse(src));
        assert!(
            !errors.iter().any(|e| matches!(
                e,
                Diagnostic::InvalidDefinition { name, .. } if name == "version"
            ) || matches!(e, Diagnostic::TooManyArguments { .. })),
            "{src:?}: {errors:?}"
        );
    }
}
