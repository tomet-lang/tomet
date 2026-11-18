//! Datetimes: formatting, order, parts and offsets.

use super::*;

fn when(text: &str) -> EvalValue {
    EvalValue::DateTime(parse_datetime(text).unwrap())
}

fn with_dates() -> EvalValue {
    map(vec![
        ("early", when("2026-01-01T00:00:00Z")),
        ("late", when("2026-06-01T12:30:00+09:00")),
    ])
}

#[test]
fn a_datetime_formats_with_the_shared_symbols() {
    assert_eq!(
        with_var("var.late.format(\"YYYY-MM-DD HH:mm\")", with_dates()).unwrap(),
        text("2026-06-01 12:30")
    );
    assert_eq!(
        with_var("var.late.format(\"YYYY年MM月DD日\")", with_dates()).unwrap(),
        text("2026年06月01日")
    );
}

#[test]
fn format_keeps_the_values_own_offset() {
    // 12:30 at +09:00 is 03:30 UTC; the value is written as it is, not converted.
    assert_eq!(
        with_var("var.late.format(\"HH\")", with_dates()).unwrap(),
        text("12")
    );
}

#[test]
fn datetimes_order_and_compare() {
    assert_eq!(
        with_var("var.early < var.late", with_dates()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.late >= var.early", with_dates()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    assert_eq!(
        with_var("var.early == var.early", with_dates()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
    // The same instant at two offsets is the same datetime.
    let same = map(vec![
        ("a", when("2026-01-01T09:00:00+09:00")),
        ("b", when("2026-01-01T00:00:00Z")),
    ]);
    assert_eq!(
        with_var("var.a == var.b", same).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
}

#[test]
fn a_datetime_does_not_order_against_a_string() {
    let mixed = map(vec![("d", when("2026-01-01T00:00:00Z"))]);
    assert_eq!(
        with_var("var.d < \"2026-02-01\"", mixed),
        Err(EvalError::NotComparable {
            left: "datetime",
            right: "string"
        })
    );
}

#[test]
fn datetimes_sort() {
    let list = map(vec![(
        "xs",
        seq(vec![
            when("2026-03-01T00:00:00Z"),
            when("2026-01-01T00:00:00Z"),
            when("2026-02-01T00:00:00Z"),
        ]),
    )]);
    assert_eq!(
        with_var(
            "var.xs.sort(d => d, \"desc\").map(d => d.format(\"MM\"))",
            list
        )
        .unwrap(),
        seq(vec![text("03"), text("02"), text("01")])
    );
}

#[test]
fn format_wants_a_string_and_a_datetime_has_no_other_members() {
    assert!(matches!(
        with_var("var.late.format(1)", with_dates()),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert_eq!(
        with_var("var.late.century", with_dates()),
        Err(EvalError::UnknownMember {
            on: "datetime",
            member: "century".to_string()
        })
    );
}

#[test]
fn a_datetime_has_its_parts_in_its_own_offset() {
    // 12:30 at +09:00: the parts are the ones written, not the UTC ones.
    for (expr, want) in [
        ("year", 2026),
        ("month", 6),
        ("day", 1),
        ("hour", 12),
        ("minute", 30),
        ("second", 0),
    ] {
        assert_eq!(
            with_var(&format!("var.late.{expr}"), with_dates()).unwrap(),
            int(want),
            "{expr}"
        );
    }
    assert_eq!(
        with_var("var.late.offset", with_dates()).unwrap(),
        text("+09:00")
    );
    assert_eq!(
        with_var("var.early.offset", with_dates()).unwrap(),
        text("+00:00")
    );
    assert_eq!(
        with_var("var.late.year == 2026", with_dates()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
}

#[test]
fn at_gives_the_same_instant_in_another_offset() {
    // 12:30 at +09:00 is 03:30 UTC.
    assert_eq!(
        with_var("var.late.at(\"Z\").format(\"HH:mm\")", with_dates()).unwrap(),
        text("03:30")
    );
    assert_eq!(
        with_var(
            "var.early.at(\"-05:30\").format(\"YYYY-MM-DD HH:mm\")",
            with_dates()
        )
        .unwrap(),
        text("2025-12-31 18:30")
    );
    assert_eq!(
        with_var("var.late.at(\"-05:00\").offset", with_dates()).unwrap(),
        text("-05:00")
    );
    // The instant is the same, so it compares equal.
    assert_eq!(
        with_var("var.late.at(\"Z\") == var.late", with_dates()).unwrap(),
        EvalValue::Scalar(Scalar::Bool(true))
    );
}

#[test]
fn at_wants_an_offset() {
    assert!(matches!(
        with_var("var.late.at(1)", with_dates()),
        Err(EvalError::TypeMismatch { got: "int", .. })
    ));
    assert!(with_var("var.late.at(\"Tokyo\")", with_dates()).is_err());
}
