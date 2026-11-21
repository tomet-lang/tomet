//! Core document structure and semantic validation.
//!
//! Validates standard and custom elements and attributes (an `@name` is looked up in the
//! element tables, a `::name` in the attribute tables, and a name found only in the other is
//! written with the wrong sigil), duplicate IDs, singletons, argument declarations, dangling
//! intra-document anchors, and type seats (`<$...>`): one outside a blueprint, or one that is not
//! a type, is an error. The header of a `.ctmt` file is held to the same attribute checks
//! ([`check_header`]).

pub mod diagnostic;
pub mod id;
pub mod seats;

pub use diagnostic::{CstValidationError, Diagnostic, Severity};
pub use id::{collect_ids, collect_ids_cst};
pub use seats::check_type_seats;

use tomet_ast::{Block, ElementView, File, Inline};
use tomet_cst::{SyntaxNode, TextRange};
use tomet_declaration::Host;
use tomet_semantics::DeclKind;
use tomet_semantics::{Bindings, Shape};
use tomet_semantics_core::is_placeholder;
use tomet_span::Span;
use tomet_tree::{Node, Place, Visit};

/// [`tomet_tree::for_each_element`], and the list items written in the combine
/// notation (`-@name(...)`) too: an item with a name is an element in a list,
/// and is held to everything an element is. A plain `- content` has no name
/// and is not passed.
fn for_each_element_and_item(doc: &File, mut f: impl FnMut(ElementView<'_>)) {
    tomet_tree::for_each_element(doc, |el| f(el.as_view()));
    tomet_tree::for_each_list_item(doc, |item| {
        if let Some(view) = item.as_element_view() {
            f(view);
        }
    });
}

/// Classifies `el` against `std` and the element tables in scope.
pub fn classify_in(
    el: ElementView<'_>,
    bindings: &Bindings,
) -> Result<tomet_semantics::ElementKind, tomet_semantics::UnknownName> {
    bindings.classify(el.name)
}

/// What a `::rule(allow: (...))` calls a plain `- item`, which has no name
/// to be called by.
pub const PLAIN_ITEM_KIND_NAME: &str = "item";

/// The name a rule's `allow:` list knows a child of a node by: an element by
/// its kind, a list item by the element it is when it has a name, and a plain
/// list item by [`PLAIN_ITEM_KIND_NAME`].
pub fn child_kind_name(
    node: tomet_tree::Node<'_>,
    bindings: &Bindings,
) -> Result<String, tomet_semantics::UnknownName> {
    match node.element_view() {
        Some(view) => Ok(classify_in(view, bindings)?.as_str().to_string()),
        None => Ok(PLAIN_ITEM_KIND_NAME.to_string()),
    }
}

pub fn shape_str(shape: Shape) -> &'static str {
    match shape {
        Shape::Block => "a block element",
        Shape::Inline => "an inline element",
    }
}

/// Runs core element, attribute, ID, argument, and type seat validations on `doc`.
pub fn validate_core(doc: &File, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    let mut seen: Vec<(String, Span)> = Vec::new();

    for_each_element_and_item(doc, |el| {
        if let Err(unknown) = classify_in(el, bindings) {
            // an attribute written as an element is not unknown, it is
            // spelled wrongly: say which spelling it takes
            errors.push(if unknown.wrong_sigil {
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
                name: el.name.to_string(),
                found: shape_str(found),
                expected: shape_str(expected),
                span: el.span,
            });
        }
    });

    check_header(&doc.header, bindings, Host::Tmt, errors);
    check_singletons(doc, bindings, errors);
    check_document_declarations(doc, errors);
    check_vocabulary_params(doc, errors);
    check_type_seats(doc, errors);
    check_arguments(doc, bindings, errors);
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

/// Holds the attributes a header writes (`::name(...)`, in a `.tmt` document or a `.ctmt` file
/// alike) to the attribute tables in scope.
///
/// A name is looked up as an attribute: a standard declaration, or an entry of the `::attribute`
/// table of the document's `::kind` (bare) or of a namespace in scope. A name found nowhere is an
/// [`Diagnostic::UnknownAttribute`]; one that is an element is a [`Diagnostic::WrongSigil`]. A
/// vocabulary's attribute is then held to what its entry declares: `host:` (where it may be
/// written), `singleton:` and `args:`. The standard declarations are checked, and counted, by
/// [`check_document_declarations`] and [`check_ctmt_declarations`].
///
/// A retired plural (`::fields`) is not unknown: it is reported as retired, with the name to
/// write instead, by those two.
///
/// A dotted name is namespaced, as an element's is: `::catasto.upstream` is the attribute
/// `upstream` of the vocabulary the document writes `catasto` for, and `::std.kind` is `::kind`.
/// A singleton is counted by what it resolves to, not by how it is written. The `::use`
/// declarations are held to [`tomet_semantics::use_problems`].
pub fn check_header(
    header: &[tomet_ast::Declaration],
    bindings: &Bindings,
    host: Host,
    errors: &mut Vec<Diagnostic>,
) {
    for (idx, problem) in tomet_semantics::use_problems(header) {
        let first = match &problem {
            tomet_semantics::UseProblem::UsedTwice { first, .. } => Some(header[*first].span),
            _ => None,
        };
        errors.push(Diagnostic::InvalidUse {
            reason: problem.to_string(),
            first,
            span: header[idx].span,
        });
    }
    let mut seen: Vec<(String, Span)> = Vec::new();
    for declaration in header {
        let name = &declaration.name;
        let span = declaration.span;
        let head = name.to_string();
        if tomet_semantics::is_written_in_std(name) && DeclKind::retired(&name.name).is_some() {
            continue;
        }
        let resolved = match bindings.classify_attribute(name) {
            Ok(kind) => kind,
            Err(unknown) => {
                errors.push(if unknown.wrong_sigil {
                    Diagnostic::WrongSigil {
                        name: unknown.name,
                        written: "::",
                        expected: "@",
                        span,
                    }
                } else {
                    Diagnostic::UnknownAttribute {
                        name: unknown.name,
                        unbound_namespace: unknown.unbound_namespace,
                        span,
                    }
                });
                continue;
            }
        };
        let Some(attribute) = bindings.attribute(name) else {
            continue;
        };
        if !attribute.host.allows(host) {
            errors.push(Diagnostic::NotForThisHost {
                declaration: head,
                only_in: attribute.host.files(),
                span,
            });
            continue;
        }
        if attribute.singleton {
            let identity = resolved.as_str().to_string();
            if let Some((_, first)) = seen.iter().find(|(seen, _)| *seen == identity) {
                errors.push(Diagnostic::DuplicateSingleton {
                    name: head.clone(),
                    first: *first,
                    duplicate: span,
                });
            } else {
                seen.push((identity, span));
            }
        }
        if !attribute.params.is_empty() {
            let args = tomet_semantics::declaration_args_view_in(declaration, bindings);
            check_params_of(&head, &attribute.params, args.as_ref(), span, errors);
        }
    }
}

/// Enforces `singleton` on the elements of the body: a vocabulary's element declared
/// `singleton: true` may be written once. The attributes of the header are counted by
/// [`check_header`].
pub fn check_singletons(doc: &File, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    use tomet_ast::{Block, Inline};

    let mut seen: Vec<(String, Span)> = Vec::new();

    let mut visit = |node: Node<'_>| {
        let span = node.span().unwrap_or_default();
        let Some(view) = node.element_view() else {
            return;
        };
        if classify_in(view, bindings).is_err() {
            // a name nothing declares: reported as unknown elsewhere
            return;
        }
        if !bindings.declaration(view.name).is_some_and(|d| d.singleton) {
            return;
        }
        let name = view.name.to_string();
        if let Some((_, first)) = seen.iter().find(|(seen_name, _)| *seen_name == name) {
            errors.push(Diagnostic::DuplicateSingleton {
                name,
                first: *first,
                duplicate: span,
            });
        } else {
            seen.push((name, span));
        }
    };

    for block in &doc.body {
        match block {
            Block::Element(_) => {
                visit(Node::Block(block));
            }
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        visit(Node::Inline(inline));
                        tomet_tree::for_each_descendant(el, false, &mut visit);
                    }
                }
            }
            Block::Section(_) | Block::List(_) => {
                visit_all(Node::Block(block), Place::Top, &mut visit);
            }
            Block::Interp(_) | Block::TypeSeat(_) => {}
            Block::Hole(_) => visit_all(Node::Block(block), Place::Top, &mut visit),
        }
    }
}

/// Everything written in `node` and under it: an element, and a list item that names one.
///
/// `(args)`, the value of a `key: value` entry and a `::connect` are left out, as a descendant walk
/// leaves them out: they describe their owner and are not part of what it holds.
fn visit_all(node: Node<'_>, place: Place, visit: &mut impl FnMut(Node<'_>)) {
    let _ = tomet_tree::walk_node(node, place, &mut |node, place| {
        if matches!(place, Place::Args | Place::PairValue | Place::Connect) {
            return Visit::<()>::Skip;
        }
        if node.element_view().is_some() {
            visit(node);
        }
        Visit::Descend
    });
}

pub fn check_unfinished(doc: &File, errors: &mut Vec<Diagnostic>) {
    for_each_element_and_item(doc, |el| {
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

fn note_text(el: ElementView<'_>) -> String {
    let Some(blocks) = el.content else {
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

pub fn check_arguments(doc: &File, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    for_each_element_and_item(doc, |el| {
        check_arguments_of(
            el.name,
            tomet_semantics::element_args_view_in(el, bindings),
            el.span,
            bindings,
            errors,
        );
    });
    tomet_tree::for_each_call(doc, |head, args, span| {
        check_call(head, args, span, bindings, errors);
    });
}

/// The arguments written for the element `name` (viewed through the slots it has),
/// held to the parameters the `::element` entry in scope declares for it.
fn check_arguments_of(
    name: &tomet_ast::Name,
    args: Option<tomet_semantics::TupleView<'_, '_>>,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    let Some(decl) = bindings.declaration(name) else {
        return;
    };
    if decl.params.is_empty() {
        return;
    }
    check_params_of(&name.to_string(), &decl.params, args.as_ref(), span, errors);
}

/// Checks a written call (`Value::Call`) against its vocabulary constructor definition when its
/// head is namespaced (`namespace.name`). Bare heads (`list`, `string`, `int`, `enum`, ...) are
/// not constructor calls and are left untouched.
pub fn check_call(
    head: &str,
    args: &tomet_ast::Tuple,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    let Some((namespace, constructor)) = head.split_once('.') else {
        return;
    };
    let Some(vocab) = bindings.vocabulary_in_scope(namespace) else {
        errors.push(Diagnostic::UnknownConstructor {
            name: head.to_string(),
            unbound_namespace: Some(namespace.to_string()),
            known: Vec::new(),
            span,
        });
        return;
    };
    let Some(decl) = vocab.constructors.get(constructor) else {
        let known = vocab.constructors.keys().cloned().collect();
        errors.push(Diagnostic::UnknownConstructor {
            name: head.to_string(),
            unbound_namespace: None,
            known,
            span,
        });
        return;
    };
    let view = tomet_semantics::constructor_args_view(args, decl);
    check_params_of(head, &decl.params, Some(&view), span, errors);
}

/// Checks every namespaced constructor call in a `.ctmt` file (`header` declarations and `body`
/// value) against the vocabularies in `bindings`.
pub fn check_ctmt_calls(
    file: &File<tomet_ast::Value>,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    for decl in &file.header {
        tomet_tree::for_each_call_in_declaration(decl, |head, args, span| {
            check_call(head, args, span, bindings, errors);
        });
    }
    tomet_tree::for_each_call_in_value(&file.body, |head, args, span| {
        check_call(head, args, span, bindings, errors);
    });
}

fn check_params_of(
    element: &str,
    params: &[tomet_semantics::ParamDecl],
    view: Option<&tomet_semantics::TupleView<'_, '_>>,
    span: Span,
    errors: &mut Vec<Diagnostic>,
) {
    use tomet_semantics::CallError;

    for problem in tomet_semantics::check_call_params(element, params, view) {
        match problem {
            CallError::DuplicateArgument { call, argument } => {
                errors.push(Diagnostic::DuplicateArgument {
                    element: call,
                    argument,
                    span,
                });
            }
            CallError::TooManyPositionalArguments { call, slots, given } => {
                errors.push(Diagnostic::TooManyPositionalArguments {
                    element: call,
                    slots,
                    given,
                    span,
                });
            }
            CallError::UnknownArgument { call, argument } => {
                errors.push(Diagnostic::UnknownArgument {
                    element: call,
                    argument,
                    span,
                });
            }
            CallError::MissingRequiredArgument { call, argument } => {
                errors.push(Diagnostic::MissingRequiredArgument {
                    element: call,
                    argument,
                    span,
                });
            }
            CallError::InvalidArgument {
                call,
                argument,
                ty,
                reason,
            } => {
                errors.push(Diagnostic::InvalidArgument {
                    element: call,
                    argument,
                    ty,
                    reason,
                    span,
                });
            }
            CallError::NotACall
            | CallError::WrongNamespace { .. }
            | CallError::UnboundNamespace { .. }
            | CallError::UnknownConstructor { .. } => {}
        }
    }
}

/// Runs checks on the standard declarations in `doc`, mapping each [`tomet_declaration::Problem`]
/// back to a [`Diagnostic`] with the declaration element's span.
pub fn check_document_declarations(doc: &File, errors: &mut Vec<Diagnostic>) {
    use tomet_declaration::{Declaration, Problem, check_declarations};

    // Every declaration written, not only the standard ones: a retired spelling is reported by
    // what it was, and the check skips a name that is nobody's.
    let (decls, spans): (Vec<_>, Vec<_>) = doc
        .header
        .iter()
        .filter_map(|written| Declaration::from_ast(written).map(|d| (d, written.span)))
        .unzip();
    for (idx, problem) in check_declarations(&decls, Host::Tmt) {
        let span = spans[idx];
        match problem {
            Problem::TooManyArguments { declaration, takes } => {
                errors.push(Diagnostic::TooManyArguments {
                    element: declaration.to_string(),
                    takes,
                    span,
                });
            }
            Problem::InvalidDefinition {
                declaration,
                name,
                reason,
            } => {
                errors.push(Diagnostic::InvalidDefinition {
                    declaration,
                    name,
                    reason,
                    span,
                });
            }
            Problem::NotForThisHost { declaration, scope } => {
                errors.push(Diagnostic::NotForThisHost {
                    declaration: declaration.to_string(),
                    only_in: scope.files(),
                    span,
                });
            }
            Problem::Repeated { declaration, first } => {
                errors.push(Diagnostic::DuplicateSingleton {
                    name: declaration.to_string(),
                    first: spans[first],
                    duplicate: span,
                });
            }
            Problem::MetaIdMoved => {
                errors.push(Diagnostic::MetaIdMoved { span });
            }
            Problem::UnexpectedData { declaration } => {
                errors.push(Diagnostic::UnexpectedDeclarationData { declaration, span });
            }
            Problem::ExpectedEntries { declaration } => {
                errors.push(Diagnostic::ExpectedEntries { declaration, span });
            }
            Problem::UnexpectedProse {
                declaration,
                reason,
            } => {
                errors.push(Diagnostic::UnexpectedDeclarationProse {
                    declaration,
                    reason,
                    span,
                });
            }
            Problem::Retired { written, instead } => {
                errors.push(Diagnostic::RetiredDeclaration {
                    written,
                    instead,
                    span,
                });
            }
        }
    }
}

/// A declaration paired with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Declared {
    pub declaration: tomet_declaration::Declaration,
    pub span: Span,
}

impl Declared {
    pub fn new(declaration: tomet_declaration::Declaration, span: Span) -> Self {
        Declared { declaration, span }
    }
}

/// Validates the standard declarations of a `.ctmt` file header against
/// [`tomet_declaration::Host::Ctmt`].
///
/// Reports host restrictions, duplicate singletons, version format errors,
/// too many arguments, and invalid macro/var definitions. Names that are not standard are
/// [`check_header`]'s: a vocabulary's attribute, or unknown.
pub fn check_ctmt_declarations(declarations: &[Declared], errors: &mut Vec<Diagnostic>) {
    use tomet_declaration::{Problem, check_declarations};

    let decls: Vec<_> = declarations.iter().map(|d| d.declaration.clone()).collect();
    for (idx, problem) in check_declarations(&decls, Host::Ctmt) {
        let span = declarations[idx].span;
        match problem {
            Problem::NotForThisHost { declaration, scope } => {
                errors.push(Diagnostic::NotForThisHost {
                    declaration: declaration.to_string(),
                    only_in: scope.files(),
                    span,
                });
            }
            Problem::Repeated { declaration, first } => {
                errors.push(Diagnostic::DuplicateSingleton {
                    name: declaration.to_string(),
                    first: declarations[first].span,
                    duplicate: span,
                });
            }
            Problem::MetaIdMoved => {
                errors.push(Diagnostic::MetaIdMoved { span });
            }
            Problem::UnexpectedData { declaration } => {
                errors.push(Diagnostic::UnexpectedDeclarationData { declaration, span });
            }
            Problem::ExpectedEntries { declaration } => {
                errors.push(Diagnostic::ExpectedEntries { declaration, span });
            }
            Problem::UnexpectedProse {
                declaration,
                reason,
            } => {
                errors.push(Diagnostic::UnexpectedDeclarationProse {
                    declaration,
                    reason,
                    span,
                });
            }
            Problem::Retired { written, instead } => {
                errors.push(Diagnostic::RetiredDeclaration {
                    written,
                    instead,
                    span,
                });
            }
            Problem::TooManyArguments { declaration, takes } => {
                errors.push(Diagnostic::TooManyArguments {
                    element: declaration.to_string(),
                    takes,
                    span,
                });
            }
            Problem::InvalidDefinition {
                declaration,
                name,
                reason,
            } => {
                errors.push(Diagnostic::InvalidDefinition {
                    declaration,
                    name,
                    reason,
                    span,
                });
            }
        }
    }
}

pub fn check_content_shape(doc: &File, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    for_each_element_and_item(doc, |el| {
        let Some(content) = el.content else { return };
        let Ok(kind) = classify_in(el, bindings) else {
            return;
        };
        let name = el.name.to_string();

        let allow = match &kind {
            tomet_semantics::ElementKind::Custom(_) => bindings
                .declaration(el.name)
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
                if !matches!(content, [] | [Block::Paragraph(_)]) {
                    errors.push(Diagnostic::ContentNotInline {
                        name,
                        span: el.span,
                    });
                }
            }
            Some(tomet_semantics::ContentAllow::Names(names)) => {
                let allowed_display: Vec<String> = names.iter().map(|n| n.to_string()).collect();
                for_each_direct_content_item(content, |desc| {
                    let Ok(desc_kind) = classify_in(desc, bindings) else {
                        return;
                    };
                    let desc_name = desc_kind.as_str().to_string();
                    if !names.iter().any(|n| n.to_string() == desc_name) {
                        errors.push(Diagnostic::DisallowedInContent {
                            name: desc_name,
                            element: name.clone(),
                            allowed: allowed_display.clone(),
                            element_span: el.span,
                            span: desc.span,
                        });
                    }
                });
            }
        }
    });
}

/// What sits directly in `content`: elements.
fn for_each_direct_content_item<'a>(content: &'a [Block], mut f: impl FnMut(ElementView<'a>)) {
    for block in content {
        match block {
            Block::Element(el) => f(el.as_view()),
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        f(el.as_view());
                    }
                }
            }
            Block::Section(_)
            | Block::List(_)
            | Block::Interp(_)
            | Block::Hole(_)
            | Block::TypeSeat(_) => {}
        }
    }
}

/// Checks how a vocabulary document declares the entries of its `::element`, `::attribute` and
/// `::constructor` tables: a field the table does not have, a value a field cannot take, a
/// `type:` that names no type, a `default:` that is not of its `type:`
/// ([`tomet_semantics::entry_problems_in`]). The same findings are reported where a vault reads
/// the vocabulary ([`tomet_semantics::LoadedVocabularies`]); this is for checking the vocabulary
/// file itself.
pub fn check_vocabulary_params(doc: &File, errors: &mut Vec<Diagnostic>) {
    for (span, problem) in tomet_semantics::entry_problems_in(doc) {
        errors.push(Diagnostic::InvalidDefinition {
            declaration: problem.table.name(),
            name: problem.name,
            reason: problem.reason,
            span,
        });
    }
}

/// Reports each top-level declaration only the vault config may write (`DeclKind::is_vault_only`).
///
/// The caller decides whether the document is allowed to write them (the vault config is; a folder
/// config and an ordinary document are not), because that takes the path, which this crate never has.
pub fn check_vault_only_declarations(doc: &File, errors: &mut Vec<Diagnostic>) {
    for declaration in &doc.header {
        if let tomet_semantics::ElementKind::Declaration(kind) =
            tomet_semantics::classify_declaration_lenient(declaration)
            && kind.is_vault_only()
        {
            errors.push(Diagnostic::VaultOnlyDeclaration {
                name: kind.name().to_string(),
                span: declaration.span,
            });
        }
    }
}

/// Checks the form of the `::meta` fields the settings declare a `type:` for.
///
/// `field_types` is the settings' `meta` declarations as `name -> type`,
/// which is all this needs of them: the validator sits below the crate that
/// reads settings, so the caller hands the declarations over as plain data.
///
/// Only a `type:` with a form can be wrong: `uuid` and `datetime`. A field the
/// document does not write, an empty one (the state in which `tomet format`
/// generates one), and a `{$ ... }` not yet evaluated or a `<$...>` not yet filled are not reported.
pub fn check_field_types(
    doc: &File,
    field_types: &std::collections::BTreeMap<String, String>,
    errors: &mut Vec<Diagnostic>,
) {
    use tomet_ast::Value;

    if field_types.is_empty() {
        return;
    }
    let mut checked_meta = false;
    let mut checked_id = false;
    for declaration in &doc.header {
        match tomet_semantics::classify_declaration_lenient(declaration) {
            tomet_semantics::ElementKind::Declaration(DeclKind::Meta) if !checked_meta => {
                checked_meta = true;
                let Some(Value::Tuple(entries)) =
                    tomet_semantics::data::declaration_entries(declaration)
                else {
                    continue;
                };
                for (field, value) in entries.named() {
                    errors.extend(field_value_error(
                        field,
                        value,
                        field_types,
                        declaration.span,
                    ));
                }
            }
            // The identity of a document is the field `id` of its vault's settings, wherever
            // it is written: `meta: { id: { type: uuid } }` holds `::id(...)` to a UUID.
            tomet_semantics::ElementKind::Declaration(DeclKind::Id) if !checked_id => {
                checked_id = true;
                let Some(value) = tomet_semantics::declaration_args_view(declaration)
                    .and_then(|args| args.get("id").cloned())
                else {
                    return;
                };
                errors.extend(field_value_error(
                    "id",
                    &value,
                    field_types,
                    declaration.span,
                ));
            }
            _ => {}
        }
    }
}

/// The diagnostic for `value` as the field `field`, if the settings declare a `type:` for the
/// field that has a form and `value` does not have it.
fn field_value_error(
    field: &str,
    value: &tomet_ast::Value,
    field_types: &std::collections::BTreeMap<String, String>,
    span: Span,
) -> Option<Diagnostic> {
    use tomet_ast::Value;
    use tomet_scalar::Scalar;
    use tomet_type::Type;

    let field_type = field_types.get(field)?;
    let ty = field_type
        .parse::<Type>()
        .ok()
        .filter(|ty| ty.has_text_form())?;
    let well_formed = match value {
        Value::Scalar(Scalar::None) => true,
        // `{$ ... }` keeps its place in the tree until it is evaluated, and `<$...>` until it is
        // filled, and what either comes to is not known here.
        _ if is_placeholder(value) => true,
        // A scalar is the text it was written as: a bare `1.10` is the
        // version `1.10`, and `2026` is not a datetime.
        scalar => scalar
            .as_scalar()
            .and_then(|s| s.text())
            .is_some_and(|text| text.is_empty() || ty.holds_text(&text) == Some(true)),
    };
    (!well_formed).then(|| Diagnostic::InvalidFieldValue {
        field: field.to_string(),
        field_type: field_type.clone(),
        value: diagnostic::written_value(value),
        span,
    })
}

pub fn check_dangling_anchors(doc: &File, errors: &mut Vec<Diagnostic>) {
    for_each_element_and_item(doc, |el| {
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
