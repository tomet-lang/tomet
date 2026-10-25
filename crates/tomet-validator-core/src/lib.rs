//! Core document structure and semantic validation.
//!
//! Validates standard and custom elements, duplicate IDs, singleton directives,
//! regions (preamble vs body), argument declarations, and dangling intra-document anchors.

pub mod diagnostic;
pub mod id;

pub use diagnostic::{CstValidationError, Diagnostic, Severity};
pub use id::{collect_ids, collect_ids_cst};

use tomet_ast::{Block, Document, Element, Inline, List};
use tomet_cst::{SyntaxNode, TextRange};
use tomet_semantics::{Bindings, Shape};

/// Classifies `el` against `std` and the namespaces in scope.
pub fn classify_in(
    el: &tomet_ast::Element,
    bindings: &Bindings,
) -> Result<tomet_semantics::ElementKind, tomet_semantics::UnknownName> {
    match &el.sigil {
        tomet_ast::Sigil::Named(name) => {
            tomet_semantics::refuse_declaration_as_element(name, bindings.classify(name)?)
        }
        _ => tomet_semantics::classify_std(el),
    }
}

pub fn shape_str(shape: Shape) -> &'static str {
    match shape {
        Shape::Block => "a block element",
        Shape::Inline => "an inline element",
    }
}

/// Runs core element, ID, region, and argument validations on `doc`.
pub fn validate_core(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    let mut seen: Vec<(String, tomet_ast::Span)> = Vec::new();

    tomet_tree::for_each_element(doc, |el| {
        if let Err(unknown) = classify_in(el, bindings) {
            // a declaration written as an element is not unknown, it is
            // spelled wrongly: say which spelling it takes
            errors.push(if unknown.declaration {
                Diagnostic::WrongSigil {
                    name: unknown.name,
                    written: "@",
                    expected: "::",
                    span: el.span,
                }
            } else {
                Diagnostic::UnknownElement {
                    name: unknown.name,
                    unbound_namespace: unknown.unbound_namespace,
                    span: el.span,
                }
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

    check_singletons_and_regions(doc, bindings, errors);
    check_retired_settings_keys(doc, errors);
    check_name_definitions(doc, errors);
    check_declared_versions(doc, errors);
    check_arguments(doc, bindings, errors);
    check_version_slot(doc, bindings, errors);
    check_unfinished(doc, errors);
    check_content_shape(doc, bindings, errors);
    check_dangling_anchors(doc, errors);

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
}

/// Enforces the `singleton` and `region` axes.
pub fn check_singletons_and_regions(
    doc: &Document,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    use tomet_ast::{Block, Inline};
    use tomet_semantics::Region;

    /// What the validator holds an element to.
    ///
    /// `region` is where the element *belongs*: a `::name` is a declaration
    /// whatever its name, so it is held to the preamble even when no
    /// vocabulary is in scope to say what it is. `declared` is what the
    /// name's own declaration says, when it can be classified at all -- the
    /// two differ exactly when the sigil is wrong.
    struct Held {
        name: String,
        singleton: bool,
        region: Region,
        declared: Option<Region>,
        document: bool,
    }

    let constraints = |el: &tomet_ast::Element| -> Option<Held> {
        let name = el.sigil.name()?;
        let document = el.sigil.is_document();
        let (singleton, declared) = match classify_in(el, bindings) {
            Ok(kind) => match bindings.declaration(name) {
                Some(decl) => (decl.singleton, Some(decl.region)),
                None => (
                    tomet_semantics::builtin_singleton(&kind),
                    Some(tomet_semantics::builtin_region(&kind)),
                ),
            },
            // a name nothing declares: reported as unknown elsewhere
            Err(_) if document => (false, None),
            Err(_) => return None,
        };
        let region = if document {
            Region::Preamble
        } else {
            declared?
        };
        Some(Held {
            name: name.to_string(),
            singleton,
            region,
            declared,
            document,
        })
    };

    let mut in_preamble = true;
    let mut seen: Vec<(String, tomet_ast::Span)> = Vec::new();

    let mut visit = |el: &tomet_ast::Element, in_place: bool| {
        let Some(Held {
            name,
            singleton,
            region,
            declared,
            document,
        }) = constraints(el)
        else {
            return;
        };
        if document && declared == Some(Region::Body) {
            errors.push(Diagnostic::WrongSigil {
                name: name.clone(),
                written: "::",
                expected: "@",
                span: el.span,
            });
        }
        if !document && declared == Some(Region::Preamble) {
            errors.push(Diagnostic::WrongSigil {
                name: name.clone(),
                written: "@",
                expected: "::",
                span: el.span,
            });
        }
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
                    .map(|held| held.region == Region::Preamble)
                    .unwrap_or(false);
                visit(el, in_preamble);
                if !still_preamble {
                    in_preamble = false;
                }
            }
            Block::Paragraph(p) => {
                for inline in &p.content {
                    match inline {
                        Inline::Element(el) => {
                            let still_preamble = constraints(el)
                                .map(|held| held.region == Region::Preamble)
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

fn validate_list_items(list: &List, visit: &mut impl FnMut(&Element, bool)) {
    for item in &list.items {
        visit(&item.element, false);
        tomet_tree::for_each_descendant(&item.element, false, |desc| visit(desc, false));
        if let Some(sub) = &item.sublist {
            validate_list_items(sub, visit);
        }
    }
}

pub fn check_unfinished(doc: &Document, errors: &mut Vec<Diagnostic>) {
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

pub fn check_arguments(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
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

/// `::kind(name, version)` and `::use(ns, version)`: a version, when written, is an
/// integer of 1 or more, and the name and the version are all they take.
///
/// `1.0` is the case this exists for: a semver habit that would read as a
/// float, and a document names only the major version.
pub fn check_version_slot(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    use tomet_ast::Value;
    use tomet_semantics::ElementKind;

    const TAKES: usize = 2;

    tomet_tree::for_each_element(doc, |el| {
        if !matches!(
            classify_in(el, bindings),
            Ok(ElementKind::Kind | ElementKind::Use)
        ) {
            return;
        }
        let Some(name) = el.sigil.name() else { return };
        let Some(Value::Map(entries)) = tomet_semantics::normalized_element_args(el) else {
            return;
        };

        // The slots take the first positional entries in order; one left over
        // after that is an argument the element has no slot for.
        if entries.iter().any(|(key, _)| key.is_empty()) {
            errors.push(Diagnostic::TooManyArguments {
                element: name.to_string(),
                takes: TAKES,
                span: el.span,
            });
        }
        if let Some((_, version)) = entries.iter().find(|(key, _)| key == "version")
            && !matches!(version, Value::Int(n) if *n >= 1)
        {
            errors.push(Diagnostic::InvalidVersion {
                element: name.to_string(),
                found: version_as_written(version),
                span: el.span,
            });
        }
    });
}

/// A version value the way a reader wrote it: strings quoted, floats with their
/// point (`1.0`, not `1`).
fn version_as_written(value: &tomet_ast::Value) -> String {
    use tomet_ast::Value;
    match value {
        Value::String(s) => format!("{s:?}"),
        Value::Float(f) => format!("{f:?}"),
        Value::Number(text) => text.clone(),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => "a value that is not a number".to_string(),
    }
}

pub fn check_content_shape(doc: &Document, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    tomet_tree::for_each_element(doc, |el| {
        let Some(content) = &el.content else { return };
        let Ok(kind) = classify_in(el, bindings) else {
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

/// Checks the versions a document declares: `::version(1.0)` and the `version:` of
/// `::vocabulary` and `::blueprint` are each a [`Type::Version`](tomet_type::Type).
///
/// `::kind(name, 2)` and `::use(ns, 3)` name only a major version, an integer,
/// and are held to that by `check_version_slot`.
pub fn check_declared_versions(doc: &Document, errors: &mut Vec<Diagnostic>) {
    use tomet_ast::Value;
    use tomet_semantics::{ElementKind, classify_std_lenient};
    use tomet_type::Type;

    tomet_tree::for_each_top_level_element(doc, |el| {
        let (declaration, written) = match classify_std_lenient(el) {
            ElementKind::Version => (
                "version",
                tomet_semantics::normalized_element_args(el)
                    .and_then(|args| match args {
                        Value::Map(entries) => entries
                            .into_iter()
                            .find(|(key, _)| key == "version")
                            .map(|(_, v)| v),
                        other => Some(other),
                    })
                    .or_else(|| {
                        tomet_semantics::data::element_data(el).and_then(|data| match data {
                            Value::Map(entries) => entries
                                .into_iter()
                                .find(|(key, _)| key == "version")
                                .map(|(_, v)| v),
                            _ => None,
                        })
                    }),
            ),
            kind @ (ElementKind::Vocabulary | ElementKind::Blueprint) => (
                if kind == ElementKind::Vocabulary {
                    "vocabulary"
                } else {
                    "blueprint"
                },
                match tomet_semantics::data::element_data(el) {
                    Some(Value::Map(entries)) => entries
                        .into_iter()
                        .find(|(key, _)| key == "version")
                        .map(|(_, v)| v),
                    _ => None,
                },
            ),
            _ => return,
        };
        let Some(written) = written else { return };
        // `${...}` is not known until it is evaluated.
        if matches!(&written, Value::Element(el) if matches!(el.sigil, tomet_ast::Sigil::Dollar)) {
            return;
        }
        if let Err(mismatch) = Type::Version.admits(&written) {
            errors.push(Diagnostic::InvalidDefinition {
                declaration,
                name: "version".to_string(),
                reason: mismatch.to_string(),
                span: el.span,
            });
        }
    });
}

/// Checks what `::macros` and `::vars` define: a macro is a string template, a
/// variable is a declaration (`type`, `default`, `about`) whose `type` is one a
/// variable can have and whose `default` is a value of it.
pub fn check_name_definitions(doc: &Document, errors: &mut Vec<Diagnostic>) {
    use tomet_ast::Value;
    use tomet_semantics::{ElementKind, VarDecl, classify_std_lenient};

    tomet_tree::for_each_top_level_element(doc, |el| match classify_std_lenient(el) {
        ElementKind::Macros => {
            let entries = [
                tomet_semantics::normalized_element_args(el),
                tomet_semantics::data::element_data(el),
            ];
            for entries in entries.into_iter().flatten() {
                let Value::Map(entries) = entries else {
                    continue;
                };
                for (name, template) in entries {
                    if !matches!(template, Value::String(_)) {
                        errors.push(Diagnostic::InvalidDefinition {
                            declaration: "macros",
                            name,
                            reason: "a macro is a string template, such as `\"https://x/${1}\"`"
                                .to_string(),
                            span: el.span,
                        });
                    }
                }
            }
        }
        ElementKind::Vars => {
            for (name, decl) in tomet_semantics::read_var_decls(el) {
                let reason = match decl {
                    Err(reason) => Some(reason),
                    Ok(decl) => var_problem(&decl),
                };
                if let Some(reason) = reason {
                    errors.push(Diagnostic::InvalidDefinition {
                        declaration: "vars",
                        name,
                        reason,
                        span: el.span,
                    });
                }
            }
        }
        _ => {}
    });

    /// A `default` that is not of the declared `type`. A `type` that is not one is
    /// refused when the declaration is read ([`tomet_semantics::VarDecl::from_value`]).
    fn var_problem(decl: &VarDecl) -> Option<String> {
        let default = decl.default.as_ref()?;
        decl.ty
            .admits(default)
            .err()
            .map(|err| format!("`default` is not a value of `type: {}`: {err}", decl.ty))
    }
}

pub fn check_retired_settings_keys(doc: &Document, errors: &mut Vec<Diagnostic>) {
    use tomet_ast::Value;

    const RETIRED: [&str; 4] = ["elements", "types", "macros", "macro"];

    tomet_tree::for_each_top_level_element(doc, |el| {
        if !el.sigil.is_bare_named("settings") && !el.sigil.is_bare_named("config") {
            return;
        }
        let Some(Value::Map(entries)) = tomet_semantics::data::element_data(el) else {
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

/// Checks the form of the `::meta` fields the settings declare a `type:` for.
///
/// `field_types` is the settings' `meta` declarations as `name -> type`,
/// which is all this needs of them: the validator sits below the crate that
/// reads settings, so the caller hands the declarations over as plain data.
///
/// Only a `type:` with a form can be wrong: `uuid` and `datetime`. A field the
/// document does not write, an empty one (the state in which `tomet format`
/// generates one), and a `${...}` not yet evaluated are not reported.
pub fn check_field_types(
    doc: &Document,
    field_types: &std::collections::BTreeMap<String, String>,
    errors: &mut Vec<Diagnostic>,
) {
    use tomet_ast::Value;
    use tomet_type::Type;

    if field_types.is_empty() {
        return;
    }
    let mut checked = false;
    tomet_tree::for_each_top_level_element(doc, |el| {
        if checked
            || tomet_semantics::classify_std_lenient(el) != tomet_semantics::ElementKind::Meta
        {
            return;
        }
        checked = true;
        let Some(Value::Map(entries)) = tomet_semantics::data::element_data(el) else {
            return;
        };
        for (field, value) in &entries {
            let Some(field_type) = field_types.get(field) else {
                continue;
            };
            let Some(ty) = Type::from_name(field_type).filter(|ty| ty.has_text_form()) else {
                continue;
            };
            let well_formed = match value {
                Value::Null => true,
                // `${...}` keeps its place in the tree until it is evaluated,
                // and what it comes to is not known here.
                Value::Element(el) if matches!(el.sigil, tomet_ast::Sigil::Dollar) => true,
                // A scalar is the text it was written as: a bare `1.10` is the
                // version `1.10`, and `2026` is not a datetime.
                scalar => scalar
                    .scalar_text()
                    .is_some_and(|text| text.is_empty() || ty.holds_text(&text) == Some(true)),
            };
            if !well_formed {
                errors.push(Diagnostic::InvalidFieldValue {
                    field: field.clone(),
                    field_type: field_type.clone(),
                    value: describe_value(value),
                    span: el.span,
                });
            }
        }
    });
}

/// A value as a reader would have written it, for a message.
fn describe_value(value: &tomet_ast::Value) -> String {
    use tomet_ast::Value;
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(n) => n.to_string(),
        Value::Number(text) => text.clone(),
        Value::Seq(_) => "a list".to_string(),
        Value::Map(_) => "a map".to_string(),
        _ => "something else".to_string(),
    }
}

pub fn check_dangling_anchors(doc: &Document, errors: &mut Vec<Diagnostic>) {
    tomet_tree::for_each_element(doc, |el| {
        let Some((_, raw_target)) = tomet_semantics::link_target_of(el) else {
            return;
        };
        let (scheme, anchor) = tomet_semantics::target_scheme(&raw_target);
        let anchor = if scheme == tomet_semantics::TargetScheme::Internal {
            anchor
        } else if scheme == tomet_semantics::TargetScheme::Document && anchor.starts_with('#') {
            &anchor[1..]
        } else {
            return;
        };
        if anchor.starts_with("item(") {
            return;
        }
        let base_id = anchor
            .split_once('.')
            .map(|(base, _)| base)
            .unwrap_or(anchor);
        if !tomet_address::has_anchor(doc, anchor) && !tomet_address::has_anchor(doc, base_id) {
            errors.push(Diagnostic::DanglingAnchor {
                anchor: anchor.to_string(),
                span: el.span,
            });
        }
    });
}

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
