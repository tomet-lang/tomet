//! A standalone document that needs the outside.

use super::*;

#[test]
fn the_same_document_without_the_promise_is_not_reported() {
    let doc = parse("see $macro.gh(1)\n");
    assert!(
        !validate_document(&doc)
            .iter()
            .any(|e| matches!(e, Diagnostic::NotStandalone { .. }))
    );
}

/// The plural spellings the singular declarations replaced say what to write instead, once, and are not
/// also reported as unknown elements.
#[test]
fn a_retired_plural_declaration_is_reported_once_with_its_replacement() {
    for (src, written, instead) in [
        ("::fields(a: (type: string))\n", "fields", "field"),
        ("::macros(m: \"x\")\n", "macros", "macro"),
        ("::vars(v: (default: 1))\n", "vars", "var"),
        ("::elements(card: ())\n", "elements", "element"),
        (
            "::vocabularies(deck: @file(d.tmt))\n",
            "vocabularies",
            "vocabulary",
        ),
        ("::blueprints(b: @file(b.tmt))\n", "blueprints", "blueprint"),
        (
            "::dictionaries(d: @file(d.tmt))\n",
            "dictionaries",
            "dictionary",
        ),
    ] {
        let errors = validate_document(&parse(src));
        assert_eq!(errors.len(), 1, "{src}: {errors:?}");
        assert!(
            matches!(&errors[0], Diagnostic::RetiredDeclaration { written: w, instead: i, .. }
                if w == written && *i == instead),
            "{src}: {errors:?}"
        );
    }
}
