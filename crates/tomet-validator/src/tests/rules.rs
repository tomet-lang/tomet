//! Rules: the descendants a node may hold, and the connects it may carry.

use super::*;

#[test]
fn a_rule_with_only_allowed_descendants_is_clean() {
    let doc =
        parse("@section[ @card(title:\"a\")[ x ] @card(title:\"b\")[ y ] ]::rule(allow:(card))\n");
    assert_eq!(rule_errors(&doc, &Bindings::default()), vec![]);
}

#[test]
fn a_disallowed_descendant_is_reported() {
    let doc = parse("@section[ @card(title:\"a\")[ x ] @heading[ y ] ]::rule(allow:(card))\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "heading"
    ));
}

#[test]
fn direct_true_does_not_recurse_past_the_first_level() {
    let doc = parse("@section[ @card[ @heading[ z ] ] ]::rule(allow:(card), direct:true)\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(
        errors,
        vec![],
        "direct:true should not see the nested heading"
    );
}

#[test]
fn without_direct_the_same_nested_heading_is_reported() {
    let doc = parse("@section[ @card[ @heading[ z ] ] ]::rule(allow:(card))\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "heading"
    ));
}

#[test]
fn a_typoed_connect_name_is_reported() {
    let doc = parse("@section[ x ]::rulle(allow:(card))\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::UnknownConnect { name, .. } if name == "rulle"
    ));
}

/// `allow:` can name a namespaced identifier (`ns.mycard`); checking
/// a descendant against it needs the document's resolved `Bindings`,
/// not just the parser's own view -- this is the scenario that
/// requires this check to live in the validator rather than earlier.
#[test]
fn allow_list_resolves_a_namespaced_custom_element_via_bindings() {
    let vocab = tomet_ast_parser::parse_file(
        "::kind(vocabulary)\n::vocabulary(ns)\n\n::element(mycard: ())\n",
    )
    .expect("vocabulary parses");
    let bindings = Bindings {
        used: std::collections::BTreeMap::from([(
            "ns".to_string(),
            tomet_semantics::Vocabulary::from_file(&vocab).expect("valid vocabulary"),
        )]),
        ..Bindings::default()
    };

    let doc = parse("@section[ @ns.mycard() @card[a] ]::rule(allow:(card, ns.mycard))\n");
    assert_eq!(rule_errors(&doc, &bindings), vec![]);
}

/// A heading's own `::rule(...)` (`Section.connects`, not an
/// `Element`'s) is checked against everything underneath the heading,
/// though a `Section` is not an `Element`.
#[test]
fn a_heading_rule_with_only_allowed_descendants_is_clean() {
    let doc = parse("=[ Notes ]::rule(allow:(card))\n@card(title:\"a\")[ x ]\n");
    assert_eq!(rule_errors(&doc, &Bindings::default()), vec![]);
}

#[test]
fn a_disallowed_descendant_under_a_heading_rule_is_reported() {
    let doc = parse("=[ Notes ]::rule(allow:(card))\n@heading[ not allowed ]\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "heading"
    ));
}

#[test]
fn a_typoed_connect_name_on_a_heading_is_reported() {
    let doc = parse("=[ Notes ]::rulle(allow:(card))\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::UnknownConnect { name, .. } if name == "rulle"
    ));
}

/// The regression this whole group guards against: `rule` itself
/// must never be classified as if it were an ordinary document
/// element, which would let the unbound-name check -- a completely
/// different check -- see `rule` and report it. Goes through
/// `validate_document`, not the rule-connect check directly: that
/// check's own tests cannot see this.
#[test]
fn a_heading_rule_connect_is_not_misclassified_as_an_unknown_element() {
    let doc = parse("=[ Notes ]::rule(allow:(card))\n@card(title:\"a\")[ x ]\n");
    let errors = validate_document(&doc);
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownElement { name, .. } if name == "rule")),
        "{errors:?}"
    );
}

#[test]
fn a_rule_allowing_item_and_task_accepts_plain_and_task_list_items() {
    let doc = parse("=[ Todo ]::rule(allow:(item, task))\n- a plain item\n-@task[ a task item ]\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors, vec![]);
}

#[test]
fn a_rule_allowing_only_task_rejects_plain_list_item() {
    let doc = parse("=[ Todo ]::rule(allow:(task))\n- a plain item\n-@task[ a task item ]\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "item"
    ));
}

#[test]
fn a_rule_allowing_only_item_rejects_task_list_item() {
    let doc = parse("=[ Todo ]::rule(allow:(item))\n- a plain item\n-@task[ a task item ]\n");
    let errors = rule_errors(&doc, &Bindings::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(matches!(
        &errors[0],
        Diagnostic::DisallowedByRule { name, .. } if name == "task"
    ));
}

#[test]
fn an_element_named_item_is_rejected_as_unknown() {
    let doc = parse("@item[ an item ]\n");
    let errors = validate_document(&doc);
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownElement { name, .. } if name == "item")),
        "{errors:?}"
    );
}

#[test]
fn an_element_named_item_is_rejected_even_in_open_vocabulary() {
    let vocab =
        tomet_ast_parser::parse_file("::kind(vocabulary)\n::vocabulary(sketch, open: true)\n")
            .expect("vocabulary parses");
    let bindings = Bindings {
        kind: Some(tomet_semantics::Vocabulary::from_file(&vocab).expect("valid vocabulary")),
        ..Bindings::default()
    };
    let doc = parse("::kind(sketch)\n\n@item[ item ]\n@widget[ widget ]\n");
    let errors = validate_document_with(&doc, &bindings);
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownElement { name, .. } if name == "widget")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, Diagnostic::UnknownElement { name, .. } if name == "item")),
        "{errors:?}"
    );
}

/// The preamble is the run of preamble-region elements at the top.
/// A misplaced declaration after body has started is rejected at parse/lowering time.
#[test]
fn a_preamble_element_after_the_body_is_reported() {
    assert!(tomet_ast_parser::parse_file("::kind(note)\n\n#[ T ]\n\n::use(deck)\n").is_err());
}
