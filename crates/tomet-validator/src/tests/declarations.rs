//! What a document's declarations are held to.

use super::*;

fn definition_problems(src: &str) -> Vec<String> {
    validate_document(&parse(src))
        .iter()
        .filter(|e| matches!(e, Diagnostic::InvalidDefinition { .. }))
        .map(|e| e.to_string())
        .collect()
}

/// A macro is a string template: a map or a number has nothing to substitute into.
#[test]
fn a_macro_that_is_not_a_string_is_reported() {
    assert!(definition_problems("::macro(\n  gh: \"https://x/$(1)\"\n)\n").is_empty());
    let found = definition_problems("::macro(\n  gh: 3\n  ok: \"x\"\n)\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`gh` in `::macro`"), "{found:?}");
}

#[test]
fn a_variable_is_a_declaration_with_a_type_it_can_have_and_a_default_that_fits() {
    let good = "::var(\n  title: (\n    type: string\n    about: | 題。\n  )\n  n: (type: int, default: 3)\n  at: (type: datetime, default: \"2026-10-25T09:00:00+09:00\")\n  tags: (type: list, default: (a, b))\n)\n";
    assert!(
        definition_problems(good).is_empty(),
        "{:?}",
        definition_problems(good)
    );

    for (src, needle) in [
        ("::var(\n  title: \"x\"\n)\n", "declared as a map"),
        ("::var(\n  title: (kind: string)\n)\n", "unknown key `kind`"),
        ("::var(\n  title: (type: person)\n)\n", "is not a type"),
        (
            "::var(\n  n: (type: int, default: \"3\")\n)\n",
            "`default` is not a value of `type: int`",
        ),
        (
            "::var(\n  at: (type: datetime, default: \"tomorrow\")\n)\n",
            "`default` is not a value of `type: datetime`",
        ),
    ] {
        let found = definition_problems(src);
        assert_eq!(found.len(), 1, "{src}: {found:?}");
        assert!(found[0].contains(needle), "{src}: {found:?}");
    }
}

/// Each concern of the vault config is a declaration of its own, and they are clean.
#[test]
fn the_config_declarations_are_clean() {
    let doc = parse(
        "::kind(config)\n::format(callout: (style: (content: block)))\n\
         ::field(created: (type: datetime))\n\
         ::blueprint(a: @file(a.blueprint.tmt))\n\
         ::vocabulary(@file(deck.vocabulary.tmt))\n\
         ::workspace(ignore: (\"x\"))\n\
         ::export(type: commonmark)\n",
    );
    let errors = validate_document(&doc);
    assert!(errors.is_empty(), "{errors:?}");
}

/// `::config` and `::settings` are not declarations: what they held is split into the declarations above.
#[test]
fn the_dissolved_config_and_settings_are_unknown_declarations() {
    for name in ["config", "settings"] {
        let doc = parse(&format!("::{name}(format: (link: (no_space: true)))\n"));
        let errors = validate_document(&doc);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                Diagnostic::UnknownAttribute { name: found, .. } if found == name
            )),
            "{name}: {errors:?}"
        );
    }
}

/// The promise of a declared `::profile(standalone)` is checked with the rest, by `validate_document`.
#[test]
fn a_standalone_document_that_needs_the_outside_is_reported() {
    let doc = parse("::profile(standalone)\n\nsee $macro.gh(1) and $(self::kind)\n");
    let errors = validate_document(&doc);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(&errors[0], Diagnostic::NotStandalone { what, .. } if what.contains("`gh`")));
}

/// Grouped declarations are held to their checks.
#[test]
fn declarations_are_checked() {
    for src in ["::field(owner: (type: person))\n"] {
        let errors = validate_document(&parse(src));
        assert_eq!(errors.len(), 1, "{src}: {errors:?}");
        assert!(
            matches!(&errors[0], Diagnostic::InvalidDefinition { declaration: "field", name, .. } if name == "owner"),
            "{src}: {errors:?}"
        );
    }
    for src in [
        "::macro(gh: \"https://x/$(1)\")\n",
        "::var(n: (type: int, default: 3))\n",
        "::workspace(ignore: (\"target\", \".git\"))\n",
        "::format(element: (table: (max_col_width: 50)), meta: (always_newline: true))\n",
    ] {
        let errors = validate_document(&parse(src));
        assert!(errors.is_empty(), "{src}: {errors:?}");
    }
}
