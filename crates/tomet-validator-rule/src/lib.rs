//! Node-level constraint and `:rule` connect validation engine.
//!
//! Enforces `:rule(allow:list(...), direct:@bool)` connect elements, verifying that
//! descendants of elements, sections, or lists comply with allowed kind specifications.

pub mod decode;

pub use decode::{RuleArgs, decode_rule_args};

use tomet_ast::{Document, Element, Span};
use tomet_semantics::Bindings;
use tomet_validator_core::{Diagnostic, classify_in};

/// Enforces every `:rule(allow:list(...))` connect found anywhere in the
/// document -- on an ordinary element's own `connects`, on a heading's
/// (`Section.connects`), and on a list's own (`List.connects`).
pub fn check_rule_connects(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    tomet_tree::for_each_element(doc, |el| {
        for connect in &el.connects {
            check_one_connect(connect, bindings, errors, |direct, f| {
                tomet_tree::for_each_descendant(el, direct, f);
            });
        }
    });
    tomet_tree::for_each_section(doc, |sec| {
        for connect in &sec.connects {
            check_one_connect(connect, bindings, errors, |direct, f| {
                tomet_tree::for_each_descendant_in_blocks(&sec.blocks, direct, f);
            });
        }
    });
    tomet_tree::for_each_list(doc, |list| {
        for connect in &list.connects {
            check_one_connect(connect, bindings, errors, |direct, f| {
                tomet_tree::for_each_item_descendant(&list.items, direct, f);
            });
        }
    });
}

/// One connect's contribution to [`check_rule_connects`], independent of
/// whether its owner is an `Element`, `Section`, or `List`.
pub fn check_one_connect(
    connect: &Element,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
    walk_descendants: impl FnOnce(bool, &mut dyn FnMut(&Element)),
) {
    let Some(connect_name) = connect.sigil.name() else {
        return;
    };
    let member = match tomet_semantics::classify_connect_member(connect_name) {
        Ok(member) => member,
        Err(unknown) => {
            errors.push(Diagnostic::UnknownConnect {
                name: unknown.name,
                span: connect.span,
            });
            return;
        }
    };
    match member {
        tomet_semantics::ConnectMember::Rule => {
            check_one_rule(connect, bindings, errors, walk_descendants);
        }
    }
}

/// `:rule(...)`'s own semantics: decode its args, then walk its owner's
/// descendants via `walk_descendants`, checking each one's classified name
/// against the rule's `allow` list.
pub fn check_one_rule(
    connect: &Element,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
    walk_descendants: impl FnOnce(bool, &mut dyn FnMut(&Element)),
) {
    let Some(rule_args) = decode_rule_args(connect) else {
        return;
    };

    check_descendants_against_rule(&rule_args, connect.span, bindings, errors, walk_descendants);
}

/// Evaluates a decoded [`RuleArgs`] against nodes reached by `walk_descendants`.
pub fn check_descendants_against_rule(
    rule_args: &RuleArgs,
    rule_span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
    walk_descendants: impl FnOnce(bool, &mut dyn FnMut(&Element)),
) {
    let allowed_display: Vec<String> = rule_args.allow.iter().map(|n| n.to_string()).collect();

    walk_descendants(rule_args.direct, &mut |descendant| {
        let kind_name = match classify_in(descendant, bindings) {
            Ok(kind) => kind.as_str().to_string(),
            Err(_) => return,
        };
        let is_allowed = rule_args
            .allow
            .iter()
            .any(|name| name.to_string() == kind_name);
        if !is_allowed {
            errors.push(Diagnostic::DisallowedByRule {
                name: kind_name,
                allowed: allowed_display.clone(),
                rule_span,
                span: descendant.span,
            });
        }
    });
}
