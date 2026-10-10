//! =[ tomet-validator ]
//!
//! Read-only `.tmt` document schema, blueprint, and lint validation.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - Read-Only In-Memory Validation:
//!   `tomet-validator` performs pure, read-only semantic and schema validation across
//!   an in-memory [`Document`]. It performs zero I/O, no reference resolution (handled
//!   by `tomet-address` and `tomet-links`), and no AST mutations.
//!
//! - Validation Rules:
//!   -| Duplicate ID Detection: Walks all element nodes and detects duplicate `id` attributes.
//!    | Unknown & Mismatched Directives: Checks element sigils against standard vocabularies and bindings.
//!    | Blueprint Conformance: Validates document structures against defined `@kind` blueprints.
//!    | Diagnostic Error Mapping: Emits structured [`Diagnostic`] items carrying exact [`TextRange`]
//!      spans, powering CLI warnings and LSP diagnostics in `apps/lsp`.

pub mod blueprint;
mod diagnostic;
mod id;
mod rule;

pub use blueprint::*;
pub use diagnostic::{CstValidationError, Diagnostic, Severity};
pub use rule::{RuleArgs, decode_rule_args};

use id::{collect_ids, collect_ids_cst};
use tomet_ast::{Block, Document, Element, Inline, List};
use tomet_cst::{SyntaxNode, TextRange};
use tomet_semantics::{Bindings, Shape};

/// Classifies `el` against `std` and the namespaces in scope.
///
/// `Sigil::Bare` and `Sigil::Dollar` carry no name and are not a
/// vocabulary question, so they go straight through.
fn classify_in(
    el: &tomet_ast::Element,
    bindings: &Bindings,
) -> Result<tomet_semantics::ElementKind, tomet_semantics::UnknownName> {
    match &el.sigil {
        tomet_ast::Sigil::Named(name) => bindings.classify(name),
        _ => tomet_semantics::classify_std(el),
    }
}

fn shape_str(shape: Shape) -> &'static str {
    match shape {
        Shape::Block => "a block element",
        Shape::Inline => "an inline element",
    }
}

/// Runs all validation rules against a parsed `Document` and returns every
/// violation found. Read-only: never mutates `doc`, never does I/O.
///
/// Knows only `std`, so every element from a vocabulary is reported as
/// unknown. Callers that can read the vault's vocabularies -- which means
/// callers that may do I/O -- should use [`validate_document_with`] and
/// pass what is in scope.
pub fn validate_document(doc: &Document) -> Vec<Diagnostic> {
    validate_document_with(doc, &Bindings::default())
}

/// [`validate_document`], with the namespaces the document has in scope.
///
/// Still does no I/O: `bindings` arrives already loaded, by whoever was
/// allowed to read the files. That split is why this can consult a
/// vocabulary without the layer below it gaining the ability to open one.
pub fn validate_document_with(doc: &Document, bindings: &Bindings) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    let mut seen: Vec<(String, tomet_ast::Span)> = Vec::new();

    // The parser deliberately accepts any well-formed name -- deciding
    // which names exist is a vocabulary question, and the parser is barred
    // from consulting one. So this is where an unknown name, or an
    // element written with the wrong shape, is reported.
    tomet_tree::for_each_element(doc, |el| {
        if let Err(unknown) = classify_in(el, bindings) {
            errors.push(Diagnostic::UnknownElement {
                name: unknown.name,
                unbound_namespace: unknown.unbound_namespace,
                span: el.span,
            });
            return;
        }
        if let Some((found, expected)) = tomet_semantics::shape_mismatch(el) {
            errors.push(Diagnostic::ShapeMismatch {
                name: el.sigil.name().map(|n| n.to_string()).unwrap_or_default(),
                found: shape_str(found),
                expected: shape_str(expected),
                span: el.span,
            });
        }
    });

    check_singletons_and_regions(doc, bindings, &mut errors);
    check_retired_settings_keys(doc, &mut errors);
    check_arguments(doc, bindings, &mut errors);
    check_unfinished(doc, &mut errors);
    check_rule_connects(doc, bindings, &mut errors);
    check_content_shape(doc, bindings, &mut errors);

    for (id, span) in collect_ids(doc) {
        if let Some((_, first)) = seen.iter().find(|(seen_id, _)| *seen_id == id) {
            errors.push(Diagnostic::DuplicateId {
                id,
                first: *first,
                duplicate: span,
            });
        } else {
            seen.push((id, span));
        }
    }

    errors
}

/// Enforces the `singleton` and `region` axes.
///
/// Both were declared in `docs/spec/builtin-settings.tmt` and read by
/// nothing, under a `placement:` key that also carried the block/inline
/// rule -- three concepts in one word, which is why that key is being
/// retired in favour of `display`, `region` and `singleton`.
///
/// The preamble is the run of preamble-region elements at the top of the
/// document. The first top-level block that is not one of them ends it,
/// and anything preamble-only after that point, or nested inside
/// content, is out of place.
fn check_singletons_and_regions(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    use tomet_ast::{Block, Inline};
    use tomet_semantics::Region;

    let constraints = |el: &tomet_ast::Element| -> Option<(String, bool, Region)> {
        let name = el.sigil.name()?;
        let kind = classify_in(el, bindings).ok()?;
        // A vocabulary says what it wants; `std` has its own table.
        let (singleton, region) = match bindings.declaration(name) {
            Some(decl) => (decl.singleton, decl.region),
            None => (
                tomet_semantics::builtin_singleton(&kind),
                tomet_semantics::builtin_region(&kind),
            ),
        };
        Some((name.to_string(), singleton, region))
    };

    let mut in_preamble = true;
    let mut seen: Vec<(String, tomet_ast::Span)> = Vec::new();

    // `in_place` is passed in rather than captured: the closure would
    // otherwise borrow `in_preamble` for the whole loop.
    let mut visit = |el: &tomet_ast::Element, in_place: bool| {
        let Some((name, singleton, region)) = constraints(el) else {
            return;
        };
        if singleton {
            if let Some((_, first)) = seen.iter().find(|(seen_name, _)| *seen_name == name) {
                errors.push(Diagnostic::DuplicateSingleton {
                    name: name.clone(),
                    first: *first,
                    duplicate: el.span,
                });
            } else {
                seen.push((name.clone(), el.span));
            }
        }
        if region == Region::Preamble && !in_place {
            errors.push(Diagnostic::OutsidePreamble {
                name,
                span: el.span,
            });
        }
    };

    for block in &doc.blocks {
        match block {
            Block::Element(el) => {
                let still_preamble = constraints(el)
                    .map(|(_, _, region)| region == Region::Preamble)
                    .unwrap_or(false);
                visit(el, in_preamble);
                if !still_preamble {
                    in_preamble = false;
                }
            }
            // A bare element that joins adjacent flow (no blank line, no
            // `;` -- `docs/spec/syntax.tmt`'s `##[ 区切り ]`) lands here
            // as `Inline::Element` instead of a top-level `Block::Element`,
            // but that is a tree-shape choice about how the parser grouped
            // it, not a statement about where it sits: it still opened its
            // own line, and `@kind`/`@meta`/... reading that column, not
            // `Placement`, is exactly what this loop already does for a
            // top-level `Block::Element` above. So each item directly in
            // the paragraph's own content is checked the same way,
            // in order, still carrying `in_preamble` forward across it --
            // only real prose (non-whitespace `Text`) or a paragraph that
            // is not still all-preamble actually ends the preamble.
            // Anything nested *inside* one of those elements' own content
            // is a different question -- that was never a place the
            // preamble could reach either way -- so it keeps the original
            // `in_place: false` treatment via `for_each_descendant`.
            Block::Paragraph(p) => {
                for inline in &p.content {
                    match inline {
                        Inline::Element(el) => {
                            let still_preamble = constraints(el)
                                .map(|(_, _, region)| region == Region::Preamble)
                                .unwrap_or(false);
                            visit(el, in_preamble);
                            if !still_preamble {
                                in_preamble = false;
                            }
                            tomet_tree::for_each_descendant(el, false, |el| visit(el, false));
                        }
                        Inline::Text(t) if !t.value.trim().is_empty() => {
                            in_preamble = false;
                        }
                        _ => {}
                    }
                }
            }
            Block::Section(s) => {
                in_preamble = false;
                // `s.connects` is deliberately not visited here -- a
                // connect is metadata about the section, not a document
                // element of its own (see `walk.rs`'s `walk_block`).
                for inline in &s.title {
                    if let Inline::Element(el) = inline {
                        visit(el, false);
                        tomet_tree::for_each_descendant(el, false, |el| visit(el, false));
                    }
                }
                for child in &s.blocks {
                    validate_section_block(child, &mut visit);
                }
            }
            Block::List(list) => {
                in_preamble = false;
                validate_list_items(list, &mut visit);
            }
        }
    }
}

fn validate_section_block(block: &Block, visit: &mut impl FnMut(&Element, bool)) {
    match block {
        Block::Element(el) => {
            visit(el, false);
            tomet_tree::for_each_descendant(el, false, |desc| visit(desc, false));
        }
        Block::Paragraph(p) => {
            for inline in &p.content {
                if let Inline::Element(el) = inline {
                    visit(el, false);
                    tomet_tree::for_each_descendant(el, false, |desc| visit(desc, false));
                }
            }
        }
        Block::Section(s) => {
            // Same exclusion as the Section arm above: `s.connects` is
            // metadata about the section, not a visitable document
            // element.
            for inline in &s.title {
                if let Inline::Element(el) = inline {
                    visit(el, false);
                    tomet_tree::for_each_descendant(el, false, |desc| visit(desc, false));
                }
            }
            for child in &s.blocks {
                validate_section_block(child, visit);
            }
        }
        Block::List(list) => validate_list_items(list, visit),
    }
}

/// [`validate_section_block`]'s counterpart for a list's own items --
/// each item's `element` (plus its descendants), recursing into a nested
/// `sublist` the same way `validate_section_block` recurses into a nested
/// `Section`. `list.connects` is deliberately not visited, the same
/// exclusion `s.connects` gets above.
fn validate_list_items(list: &List, visit: &mut impl FnMut(&Element, bool)) {
    for item in &list.items {
        visit(&item.element, false);
        tomet_tree::for_each_descendant(&item.element, false, |desc| visit(desc, false));
        if let Some(sub) = &item.sublist {
            validate_list_items(sub, visit);
        }
    }
}

/// Surfaces `@draft`, `@fixme`, and `@conflict` -- the document's own
/// statement that something here still needs attention, whether that is
/// missing/wrong prose or an unresolved merge conflict.
///
/// Warnings, so `tomet check` reports them and still passes: marking a
/// gap (or leaving a conflict for someone else to resolve) has to be
/// cheaper than hiding it, or nobody marks it.
///
/// `classify_std` rather than a name comparison, so a `@ns.draft` from some
/// vocabulary is not mistaken for `std`'s.
fn check_unfinished(doc: &Document, errors: &mut Vec<Diagnostic>) {
    tomet_tree::for_each_element(doc, |el| {
        let kind = match tomet_semantics::classify_std(el) {
            Ok(kind) => kind,
            Err(_) => return,
        };
        let note = note_text(el);
        match kind {
            tomet_semantics::ElementKind::Draft => errors.push(Diagnostic::Draft {
                note,
                span: el.span,
            }),
            tomet_semantics::ElementKind::Fixme => errors.push(Diagnostic::Fixme {
                note,
                span: el.span,
            }),
            tomet_semantics::ElementKind::Conflict => {
                errors.push(Diagnostic::Conflict { span: el.span })
            }
            _ => {}
        }
    });
}

/// The plain text of an element's `[content]`, which is where the note
/// about what is missing lives. Nested elements are skipped: a note is
/// prose, and anything richer belongs in the document rather than in the
/// marker.
fn note_text(el: &tomet_ast::Element) -> String {
    let Some(blocks) = el.content.as_ref() else {
        return String::new();
    };
    let mut out = String::new();
    for block in blocks {
        let tomet_ast::Block::Paragraph(p) = block else {
            continue;
        };
        for inline in &p.content {
            if let tomet_ast::Inline::Text(t) = inline {
                out.push_str(&t.value);
            }
        }
    }
    out.trim().to_string()
}

/// Checks each element's `(args)` against the `@param`s its vocabulary
/// declares.
///
/// Silent for anything `std` owns, and for a custom element whose
/// declaration carries no `@args`. An absent `@args` says nothing about
/// the arguments rather than saying there are none -- that second
/// statement is `@data`'s `open: false`, which has no `(args)` twin yet,
/// and inventing one here would decide it by accident.
///
/// The arguments are normalized first, so a positional value has already
/// been moved onto the slot its `@param` declares. Without that,
/// `@deck.card(3)` would report the sentinel-keyed entry as an unknown
/// argument named `""`.
fn check_arguments(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    use tomet_ast::Value;

    tomet_tree::for_each_element(doc, |el| {
        let Some(name) = el.sigil.name() else { return };
        let Some(decl) = bindings.declaration(name) else {
            return;
        };
        if decl.params.is_empty() {
            return;
        }

        let args = tomet_semantics::normalized_element_args_in(el, bindings);
        let entries: &[(String, Value)] = match args.as_ref() {
            Some(Value::Map(entries)) => entries,
            // A non-map `(args)` survives normalization only when the
            // element has no positional slot to put it on, which cannot
            // happen here: `decl.params` is non-empty. Anything else is
            // `(args)` being absent.
            _ => &[],
        };

        for (key, _) in entries {
            if !key.is_empty() && decl.param(key).is_none() {
                errors.push(Diagnostic::UnknownArgument {
                    element: name.to_string(),
                    argument: key.clone(),
                    span: el.span,
                });
            }
        }
        for param in decl.params.iter().filter(|p| p.required) {
            if !entries.iter().any(|(k, _)| *k == param.name) {
                errors.push(Diagnostic::MissingRequiredArgument {
                    element: name.to_string(),
                    argument: param.name.clone(),
                    span: el.span,
                });
            }
        }
    });
}

/// Enforces every `:rule(allow:list(...))` connect found anywhere in the
/// document -- on an ordinary element's own `connects`, on a heading's
/// (`Section.connects`, since a `Section` is not an `Element` and so is
/// never reached by the `el` loop below), and on a list's own
/// (`List.connects`, likewise never reached by either of the other two
/// loops) -- and reports an unrecognized connect name (`:xxx(...)` where
/// `xxx` is not in `tomet_semantics::CONNECT_MEMBERS`).
///
/// Needs `bindings`, unlike the parser or `tomet-address`:
/// `allow:list(ns.mycard)` can name a namespaced identifier, and deciding
/// whether a descendant actually *is* `ns.mycard` requires the document's
/// resolved vocabulary. This is why the check lives here rather than
/// earlier in the pipeline.
///
/// Only iterates each visited element's/section's/list's own `connects`
/// -- never recurses into a connect's internals beyond that, and
/// `tomet_tree::for_each_element`/`for_each_section`/`for_each_list`
/// themselves never descend into `connects` either (see
/// `for_each_descendant`'s doc comment), so a connect element is never
/// misclassified as an ordinary document element.
fn check_rule_connects(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
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
/// whether its owner is an `Element` or a `Section`: classify its name,
/// then dispatch to the one implemented member (`rule`). `walk_descendants`
/// is how the caller's owner-specific descendants get reached once
/// `check_one_rule` has decided (from `connect`'s own `direct:` arg)
/// whether that means immediate children only or every depth.
fn check_one_connect(
    connect: &tomet_ast::Element,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
    walk_descendants: impl FnOnce(bool, &mut dyn FnMut(&tomet_ast::Element)),
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
/// descendants (or just its immediate children, if `direct:true`) via
/// `walk_descendants`, checking each one's classified name against the
/// rule's `allow` list.
///
/// Compares against the *classified* name (`ElementKind::as_str()`), not
/// the name as literally written -- these agree for `std` names and for
/// an explicitly namespaced one (`ns.mycard`), which covers everything
/// this MVP's own examples use. They can diverge for a bare name that
/// resolves through the document's *own* `@kind` vocabulary, which
/// `Bindings::classify` normalizes to `namespace.name` even though it was
/// written bare -- a pre-existing quirk of that classification (not
/// introduced here), not yet worth a special case until real usage shows
/// it matters.
fn check_one_rule(
    connect: &tomet_ast::Element,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
    walk_descendants: impl FnOnce(bool, &mut dyn FnMut(&tomet_ast::Element)),
) {
    let Some(rule_args) = decode_rule_args(connect) else {
        // Malformed `:rule(...)` args -- an MVP-scoped, author-confirmed
        // no-op (see `rule::decode_rule_args`'s doc comment).
        return;
    };

    let allowed_display: Vec<String> = rule_args.allow.iter().map(|n| n.to_string()).collect();

    walk_descendants(rule_args.direct, &mut |descendant| {
        let kind_name = match classify_in(descendant, bindings) {
            Ok(kind) => kind.as_str().to_string(),
            // An unknown descendant is `check_arguments`'/the top-level
            // loop's diagnostic to report, not this one's -- avoid
            // reporting the same element twice under two different
            // rules.
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
                rule_span: connect.span,
                span: descendant.span,
            });
        }
    });
}

/// Checks every element's `[content]`/`|content` against its
/// content-shape rule: `tomet_semantics::builtin_content_shape` for a
/// built-in kind, or a vocabulary's own `@content{allow:}`
/// (`ElementDecl::content_allow`) for a custom one. No rule (`None`
/// either way) means no check, same "absent says nothing" reasoning
/// `@data`'s `open:` already has.
fn check_content_shape(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    tomet_tree::for_each_element(doc, |el| {
        let Some(content) = &el.content else { return };
        let Ok(kind) = classify_in(el, bindings) else {
            // Unknown name is the top-level loop's diagnostic, not this
            // one's.
            return;
        };
        let name = el.sigil.name().map(|n| n.to_string()).unwrap_or_default();

        let allow = match &kind {
            tomet_semantics::ElementKind::Custom(_) => el
                .sigil
                .name()
                .and_then(|n| bindings.declaration(n))
                .and_then(|decl| decl.content_allow.clone()),
            _ => match tomet_semantics::builtin_content_shape(&kind) {
                Some(tomet_semantics::ContentShape::Inline) => {
                    Some(tomet_semantics::ContentAllow::Inline)
                }
                Some(tomet_semantics::ContentShape::Block) | None => None,
            },
        };

        match allow {
            None | Some(tomet_semantics::ContentAllow::Any) => {}
            Some(tomet_semantics::ContentAllow::Inline) => {
                if !matches!(content.as_slice(), [] | [Block::Paragraph(_)]) {
                    errors.push(Diagnostic::ContentNotInline {
                        name,
                        span: el.span,
                    });
                }
            }
            Some(tomet_semantics::ContentAllow::Names(names)) => {
                let allowed_display: Vec<String> = names.iter().map(|n| n.to_string()).collect();
                for_each_direct_content_element(content, |descendant| {
                    let Ok(desc_kind) = classify_in(descendant, bindings) else {
                        return;
                    };
                    let desc_name = desc_kind.as_str().to_string();
                    if !names.iter().any(|n| n.to_string() == desc_name) {
                        errors.push(Diagnostic::DisallowedInContent {
                            name: desc_name,
                            element: name.clone(),
                            allowed: allowed_display.clone(),
                            element_span: el.span,
                            span: descendant.span,
                        });
                    }
                });
            }
        }
    });
}

/// Visits the immediate items of `content` that are elements -- a
/// `Block::Element` directly, or an `Inline::Element` sitting inside a
/// `Block::Paragraph` (an element placed *inline* rather than at its own
/// line start, e.g. `@outer[ @mid[...] ]` all on one line). Deliberately
/// one level only, unlike `:rule`'s own default: a nested element found
/// here has its *own* `@content{allow:}` (if any) checked independently,
/// the next time [`check_content_shape`]'s own `for_each_element` reaches
/// it -- recursing into it here too would make an outer `allow:` list
/// double as a rule for content several elements removed from it, which
/// nothing asked for.
fn for_each_direct_content_element<'a>(
    content: &'a [Block],
    mut f: impl FnMut(&'a tomet_ast::Element),
) {
    for block in content {
        match block {
            Block::Element(el) => f(el),
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        f(el);
                    }
                }
            }
            Block::Section(_) => {}
            Block::List(_) => {}
        }
    }
}

/// Reports `@settings`/`@config` keys that have been retired.
///
/// One key so far, `elements:`, plus the `types:` map that sat beside it.
/// Between them they described a custom element -- its arguments, whether
/// it was a singleton, which shape it took -- which is a `@vocabulary`
/// question now. See [`Diagnostic::RetiredSettingsKey`] for why this
/// is an error and not a silent skip.
///
/// Top-level per `for_each_top_level_element`, not a raw `doc.blocks`
/// walk: `@settings`/`@config` is a directive (`is_directive`), exempt
/// from `shape_mismatch` when it joins an adjacent paragraph while still
/// at column 1 (`docs/spec/syntax.tmt`'s `##[ 区切り ]`) -- so an
/// `@kind(settings)` immediately followed by `@settings(...)`, the
/// ordinary shape of a settings file, is silent there and must still be
/// read here.
fn check_retired_settings_keys(doc: &Document, errors: &mut Vec<Diagnostic>) {
    use tomet_ast::Value;

    const RETIRED: [&str; 2] = ["elements", "types"];

    tomet_tree::for_each_top_level_element(doc, |el| {
        if !el.sigil.is_bare_named("settings") && !el.sigil.is_bare_named("config") {
            return;
        }
        let Some(Value::Map(entries)) = tomet_semantics::embedded::element_data(el) else {
            return;
        };
        for key in RETIRED {
            if entries.iter().any(|(k, _)| k == key) {
                errors.push(Diagnostic::RetiredSettingsKey {
                    key: key.to_string(),
                    span: el.span,
                });
            }
        }
    });
}

/// Runs all validation rules directly against a Concrete Syntax Tree ([`SyntaxNode`])
/// and returns violations with exact byte [`TextRange`]s.
pub fn validate_cst(root: &SyntaxNode) -> Vec<CstValidationError> {
    let mut errors = Vec::new();
    let mut seen: Vec<(String, TextRange)> = Vec::new();

    for (id, range) in collect_ids_cst(root) {
        if let Some((_, first_range)) = seen.iter().find(|(seen_id, _)| *seen_id == id) {
            errors.push(CstValidationError::DuplicateId {
                id,
                first_range: *first_range,
                duplicate_range: range,
            });
        } else {
            seen.push((id, range));
        }
    }

    errors
}

#[cfg(test)]
mod tests;
