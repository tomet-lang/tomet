//! Cross-crate coverage for `:rule(allow:list(...), direct:@bool)`: the
//! `tests/fixtures/rule/*.tmt` fixtures are already exercised for parsing
//! and rendering by `corpus`/`snapshot` (`:rule` renders invisibly
//! everywhere, so those targets alone cannot tell a passing document
//! from a violating one) -- this is where the actual validator
//! diagnostics get asserted.

use std::path::Path;

use tomet_tests::read_fixture;
use tomet_validator::{Diagnostic, validate_document};

fn parse_fixture(name: &str) -> tomet_ast::Document {
    let src = read_fixture(Path::new("rule").join(name).as_path());
    tomet_parser::parse_document(&src).expect("fixture parses")
}

#[test]
fn passing_fixture_has_no_rule_diagnostics() {
    let doc = parse_fixture("passing.tmt");
    let errors = validate_document(&doc);
    let rule_errors: Vec<_> = errors
        .iter()
        .filter(|e| {
            matches!(
                e,
                Diagnostic::DisallowedByRule { .. } | Diagnostic::UnknownConnect { .. }
            )
        })
        .collect();
    assert_eq!(rule_errors, Vec::<&Diagnostic>::new(), "{errors:?}");
}

#[test]
fn violating_fixture_reports_the_disallowed_heading() {
    let doc = parse_fixture("violating.tmt");
    let errors = validate_document(&doc);
    let rule_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, Diagnostic::DisallowedByRule { .. }))
        .collect();
    assert_eq!(rule_errors.len(), 1, "{errors:?}");
    assert!(matches!(
        rule_errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "heading"
    ));
}

#[test]
fn unknown_connect_fixture_reports_the_typo() {
    let doc = parse_fixture("unknown-connect.tmt");
    let errors = validate_document(&doc);
    let rule_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, Diagnostic::UnknownConnect { .. }))
        .collect();
    assert_eq!(rule_errors.len(), 1, "{errors:?}");
    assert!(matches!(
        rule_errors[0],
        Diagnostic::UnknownConnect { name, .. } if name == "rulle"
    ));
}

#[test]
fn heading_connect_fixture_has_no_rule_diagnostics() {
    let doc = parse_fixture("heading-connect.tmt");
    let errors = validate_document(&doc);
    let rule_errors: Vec<_> = errors
        .iter()
        .filter(|e| {
            matches!(
                e,
                Diagnostic::DisallowedByRule { .. }
                    | Diagnostic::UnknownConnect { .. }
                    // Closes the blind spot that let a heading's own
                    // `:rule(...)` report a false `unknown element
                    // \`rule\`` go unnoticed: `Section.connects` used to
                    // reach the generic "is this name bound" check via
                    // `walk.rs`'s `walk_block`, same as an ordinary
                    // element's `connects` never does. This filter did
                    // not check for that diagnostic before, so this
                    // fixture staying green never meant this case was
                    // actually clean.
                    | Diagnostic::UnknownElement { .. }
            )
        })
        .collect();
    assert_eq!(rule_errors, Vec::<&Diagnostic>::new(), "{errors:?}");
}

#[test]
fn heading_descendants_fixture_reports_the_disallowed_one() {
    // Unlike `heading-connect.tmt` (the connect alone, nothing under the
    // heading to check -- a vacuous pass even before the fix below), this
    // fixture has real descendants, so it actually exercises the rule's
    // allow-list against a heading's own `section.blocks` --
    // `check_rule_connects` used to only ever reach an `Element`'s own
    // `connects`, and a `Section` is not an `Element`, so a heading's
    // `:rule(...)` never checked anything at all before this fixture's
    // task fixed that.
    let doc = parse_fixture("heading-descendants.tmt");
    let errors = validate_document(&doc);
    let rule_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, Diagnostic::DisallowedByRule { .. }))
        .collect();
    assert_eq!(rule_errors.len(), 1, "{errors:?}");
    assert!(matches!(
        rule_errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "heading"
    ));
}
