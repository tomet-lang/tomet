//! Core document structure and semantic validation.
//!
//! Validates standard and custom elements, duplicate IDs, singleton directives,
//! regions (preamble vs body), argument declarations, and dangling intra-document anchors.

pub mod diagnostic;
pub mod id;

pub use diagnostic::{CstValidationError, Diagnostic, Severity};
pub use id::{collect_ids, collect_ids_cst};

use tomet_ast::{Block, ElementView, File, Inline};
use tomet_span::Span;
use tomet_cst::{SyntaxNode, TextRange};
use tomet_semantics::DeclKind;
use tomet_semantics::{Bindings, Shape};
use tomet_semantics_core::admits::admits;
use tomet_tree::{Node, Place, Visit};

/// Something written in the document that a region or sigil rule applies to:
/// an element, or a declaration.
#[derive(Clone, Copy)]
enum Written<'a> {
    Element(ElementView<'a>),
    Declaration(&'a tomet_ast::Declaration),
}

impl Written<'_> {
    fn name(&self) -> Option<&tomet_ast::Name> {
        match self {
            Written::Element(el) => Some(el.name),
            Written::Declaration(d) => Some(&d.name),
        }
    }

    fn span(&self) -> Span {
        match self {
            Written::Element(el) => el.span,
            Written::Declaration(d) => d.span,
        }
    }

    /// Whether it was written `::name`.
    fn is_declaration(&self) -> bool {
        matches!(self, Written::Declaration(_))
    }
}

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

/// Classifies `el` against `std` and the namespaces in scope.
pub fn classify_in(
    el: ElementView<'_>,
    bindings: &Bindings,
) -> Result<tomet_semantics::ElementKind, tomet_semantics::UnknownName> {
    tomet_semantics::refuse_declaration_as_element(el.name, bindings.classify(el.name)?)
}

/// What a `:rule(allow: list(...))` calls a plain `- item`, which has no name
/// to be called by.
pub const PLAIN_ITEM_KIND_NAME: &str = "bare";

/// The name a rule's `allow:` list knows a child of a node by: an element by
/// its kind, a list item by the element it is when it has a name, and a plain
/// list item by [`PLAIN_ITEM_KIND_NAME`].
pub fn child_kind_name(
    child: tomet_tree::Child<'_>,
    bindings: &Bindings,
) -> Result<String, tomet_semantics::UnknownName> {
    let kind = match child {
        tomet_tree::Child::Element(el) => classify_in(el.as_view(), bindings)?,
        tomet_tree::Child::Item(item) => match item.as_element_view() {
            Some(view) => classify_in(view, bindings)?,
            None => return Ok(PLAIN_ITEM_KIND_NAME.to_string()),
        },
    };
    Ok(kind.as_str().to_string())
}

pub fn shape_str(shape: Shape) -> &'static str {
    match shape {
        Shape::Block => "a block element",
        Shape::Inline => "an inline element",
    }
}

/// Runs core element, ID, region, and argument validations on `doc`.
pub fn validate_core(doc: &File, bindings: &Bindings, errors: &mut Vec<Diagnostic>) {
    let mut seen: Vec<(String, Span)> = Vec::new();

    for_each_element_and_item(doc, |el| {
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
                name: el.name.to_string(),
                found: shape_str(found),
                expected: shape_str(expected),
                span: el.span,
            });
        }
    });

    tomet_tree::for_each_declaration(doc, |declaration| {
        // A declaration is held to `std` alone: it is spelled `::`, so it is not
        // looked up in the vocabularies in scope.
        if let Err(unknown) = tomet_semantics::classify_declaration(declaration) {
            errors.push(Diagnostic::UnknownElement {
                name: unknown.name,
                unbound_namespace: unknown.unbound_namespace,
                span: declaration.span,
            });
        }
    });

    check_singletons_and_regions(doc, bindings, errors);
    check_document_declarations(doc, errors);
    check_vocabulary_params(doc, errors);
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

/// Enforces the `singleton` and `region` axes.
pub fn check_singletons_and_regions(
    doc: &File,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    use tomet_ast::{Block, Inline};
    use tomet_semantics::Region;

    /// What the validator holds an element to.
    struct Held {
        name: String,
        singleton: bool,
        declared: Option<Region>,
        document: bool,
    }

    let constraints = |written: Written<'_>| -> Option<Held> {
        let name = written.name()?;
        let document = written.is_declaration();
        let classified = match written {
            Written::Element(el) => classify_in(el, bindings),
            Written::Declaration(d) => tomet_semantics::classify_declaration(d),
        };
        let (singleton, declared) = match classified {
            Ok(kind) => match bindings.declaration(name) {
                Some(decl) => (decl.singleton, Some(decl.region)),
                None => (
                    false, // Standard declarations are counted in one place by `check_declarations`
                    Some(tomet_semantics::builtin_region(&kind)),
                ),
            },
            // a name nothing declares: reported as unknown elsewhere
            Err(_) if document => (false, None),
            Err(_) => return None,
        };
        Some(Held {
            name: name.to_string(),
            singleton,
            declared,
            document,
        })
    };

    let mut seen: Vec<(String, Span)> = Vec::new();

    let mut visit = |written: Written<'_>| {
        let span = written.span();
        let Some(Held {
            name,
            singleton,
            declared,
            document,
        }) = constraints(written)
        else {
            return;
        };
        if document && declared == Some(Region::Body) {
            errors.push(Diagnostic::WrongSigil {
                name: name.clone(),
                written: "::",
                expected: "@",
                span,
            });
        }
        if !document && declared == Some(Region::Preamble) {
            errors.push(Diagnostic::WrongSigil {
                name: name.clone(),
                written: "@",
                expected: "::",
                span,
            });
        }
        if singleton {
            if let Some((_, first)) = seen.iter().find(|(seen_name, _)| *seen_name == name) {
                errors.push(Diagnostic::DuplicateSingleton {
                    name: name.clone(),
                    first: *first,
                    duplicate: span,
                });
            } else {
                seen.push((name.clone(), span));
            }
        }
    };

    for decl in &doc.header {
        visit(Written::Declaration(decl));
    }

    for block in &doc.body {
        match block {
            Block::Element(el) => {
                visit(Written::Element(el.as_view()));
            }
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        visit(Written::Element(el.as_view()));
                        tomet_tree::for_each_descendant(el, false, |child| {
                            visit_child(child, &mut visit)
                        });
                    }
                }
            }
            Block::Section(_) | Block::List(_) => {
                visit_all(Node::Block(block), Place::Top, &mut visit);
            }
            Block::Interp(_) => {}
            Block::Hole(_) => visit_all(Node::Block(block), Place::Top, &mut visit),
        }
    }
}

/// Everything written in `node` and under it: an element, and a list item that names one.
///
/// `(args)`, the value of a `key: value` entry and a `:connect` are left out, as a descendant walk
/// leaves them out: they describe their owner and are not part of what it holds.
fn visit_all(node: Node<'_>, place: Place, visit: &mut impl FnMut(Written<'_>)) {
    let _ = tomet_tree::walk_node(node, place, &mut |node, place| {
        if matches!(place, Place::Args | Place::PairValue | Place::Connect) {
            return Visit::<()>::Skip;
        }
        match node {
            Node::Item(item) => visit_child(tomet_tree::Child::Item(item), visit),
            other => {
                if let Some(el) = other.element() {
                    visit(Written::Element(el.as_view()));
                }
            }
        }
        Visit::Descend
    });
}

/// What a child is held to: an element as itself, a list item as the element it
/// is when it has a name, and nothing when it has none.
fn visit_child(child: tomet_tree::Child<'_>, visit: &mut impl FnMut(Written<'_>)) {
    match child {
        tomet_tree::Child::Element(el) => visit(Written::Element(el.as_view())),
        tomet_tree::Child::Item(item) => {
            if let Some(view) = item.as_element_view() {
                visit(Written::Element(view));
            }
        }
    }
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
            tomet_semantics::normalized_element_args_in(el, bindings),
            el.span,
            bindings,
            errors,
        );
    });
    for declaration in &doc.header {
        check_arguments_of(
            &declaration.name,
            tomet_semantics::normalized_declaration_args_in(declaration, bindings),
            declaration.span,
            bindings,
            errors,
        );
    }
}

/// The arguments written for `name` (already bound to the slots it has),
/// held to the parameters a vocabulary in scope declares for it.
fn check_arguments_of(
    name: &tomet_ast::Name,
    args: Option<tomet_ast::Value>,
    span: Span,
    bindings: &Bindings,
    errors: &mut Vec<Diagnostic>,
) {
    use tomet_ast::Value;

    let Some(decl) = bindings.declaration(name) else {
        return;
    };
    if decl.params.is_empty() {
        return;
    }

    let entries: &[(String, Value)] = match args.as_ref() {
        Some(Value::Map(entries)) => entries,
        _ => &[],
    };

    for (key, _) in entries {
        if !key.is_empty() && decl.param(key).is_none() {
            errors.push(Diagnostic::UnknownArgument {
                element: name.to_string(),
                argument: key.clone(),
                span,
            });
        }
    }
    // An argument is held to the `type:` its parameter declares. A parameter
    // whose `type:` names no type is not checked (the vocabulary says so
    // where it is read), and a `${...}` is not known until it is evaluated.
    for (key, value) in entries {
        let Some(ty) = decl.param(key).and_then(|p| p.declared_type()) else {
            continue;
        };
        if matches!(value, Value::Interp(_)) {
            continue;
        }
        if let Err(mismatch) = admits(ty, value) {
            errors.push(Diagnostic::InvalidArgument {
                element: name.to_string(),
                argument: key.clone(),
                ty: ty.to_string(),
                reason: mismatch.to_string(),
                span,
            });
        }
    }
    for param in decl.params.iter().filter(|p| p.required) {
        if !entries.iter().any(|(k, _)| *k == param.name) {
            errors.push(Diagnostic::MissingRequiredArgument {
                element: name.to_string(),
                argument: param.name.clone(),
                span,
            });
        }
    }
}

/// Runs checks on the standard declarations in `doc`, mapping each [`Problem`]
/// back to a [`Diagnostic`] with the declaration element's span.
pub fn check_document_declarations(doc: &File, errors: &mut Vec<Diagnostic>) {
    use tomet_declaration::{Host, Problem, check_declarations, declarations};

    let found = declarations(doc);
    let decls: Vec<_> = found.iter().map(|f| f.declaration.clone()).collect();
    for (idx, problem) in check_declarations(&decls, Host::Tmt) {
        let span = found[idx].span;
        match problem {
            Problem::InvalidVersion { declaration, found } => {
                errors.push(Diagnostic::InvalidVersion {
                    element: declaration.to_string(),
                    found,
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
            Problem::NotForThisHost { declaration } => {
                errors.push(Diagnostic::NotForThisHost { declaration, span });
            }
            Problem::Repeated { declaration, first } => {
                errors.push(Diagnostic::DuplicateSingleton {
                    name: declaration.to_string(),
                    first: found[first].span,
                    duplicate: span,
                });
            }
            Problem::MetaIdMoved => {
                errors.push(Diagnostic::MetaIdMoved { span });
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

/// Validates declarations from a `.ctmt` file header against [`Host::Ctmt`].
///
/// Reports host restrictions, duplicate singletons, version format errors,
/// too many arguments, and invalid macro/var definitions. Declarations whose
/// names are not standard are ignored (their meaning belongs to the tool that
/// reads the file).
pub fn check_ctmt_declarations(declarations: &[Declared], errors: &mut Vec<Diagnostic>) {
    use tomet_declaration::{Host, Problem, check_declarations};

    let decls: Vec<_> = declarations.iter().map(|d| d.declaration.clone()).collect();
    for (idx, problem) in check_declarations(&decls, Host::Ctmt) {
        let span = declarations[idx].span;
        match problem {
            Problem::NotForThisHost { declaration } => {
                errors.push(Diagnostic::NotForThisHost { declaration, span });
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
            Problem::InvalidVersion { declaration, found } => {
                errors.push(Diagnostic::InvalidVersion {
                    element: declaration.to_string(),
                    found,
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
                for_each_direct_content_item(content, |descendant| {
                    let classified = match descendant {
                        Written::Element(desc) => classify_in(desc, bindings),
                        Written::Declaration(desc) => tomet_semantics::classify_declaration(desc),
                    };
                    let Ok(desc_kind) = classified else {
                        return;
                    };
                    let desc_name = desc_kind.as_str().to_string();
                    if !names.iter().any(|n| n.to_string() == desc_name) {
                        errors.push(Diagnostic::DisallowedInContent {
                            name: desc_name,
                            element: name.clone(),
                            allowed: allowed_display.clone(),
                            element_span: el.span,
                            span: descendant.span(),
                        });
                    }
                });
            }
        }
    });
}

/// What sits directly in `content`: elements.
fn for_each_direct_content_item<'a>(content: &'a [Block], mut f: impl FnMut(Written<'a>)) {
    for block in content {
        match block {
            Block::Element(el) => f(Written::Element(el.as_view())),
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        f(Written::Element(el.as_view()));
                    }
                }
            }
            Block::Section(_) | Block::List(_) | Block::Interp(_) | Block::Hole(_) => {}
        }
    }
}

/// Checks how a vocabulary document declares its parameters: a `type:` that names
/// no type, a `default:` that is not of its `type:`. The same findings are reported
/// where a vault reads the vocabulary ([`tomet_semantics::LoadedVocabularies`]);
/// this is for checking the vocabulary file itself.
pub fn check_vocabulary_params(doc: &File, errors: &mut Vec<Diagnostic>) {
    use tomet_semantics::{ElementKind, Vocabulary, classify_declaration_lenient};

    let Some(vocab) = Vocabulary::from_file(doc) else {
        return;
    };
    let mut elements_span = None;
    for declaration in &doc.header {
        if elements_span.is_none()
            && classify_declaration_lenient(declaration)
                == ElementKind::Declaration(DeclKind::Elements)
        {
            elements_span = Some(declaration.span);
        }
    }
    let Some(span) = elements_span else { return };
    for problem in vocab.param_problems() {
        errors.push(Diagnostic::InvalidDefinition {
            declaration: "elements",
            name: format!("{}.{}", problem.element, problem.param),
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
/// generates one), and a `${...}` not yet evaluated are not reported.
pub fn check_field_types(
    doc: &File,
    field_types: &std::collections::BTreeMap<String, String>,
    errors: &mut Vec<Diagnostic>,
) {
    use tomet_ast::Value;
    use tomet_tree::ValueExt;

    if field_types.is_empty() {
        return;
    }
    let mut checked_meta = false;
    let mut checked_id = false;
    for declaration in &doc.header {
        match tomet_semantics::classify_declaration_lenient(declaration) {
            tomet_semantics::ElementKind::Declaration(DeclKind::Meta) if !checked_meta => {
                checked_meta = true;
                let Some(Value::Map(entries)) =
                    tomet_semantics::data::declaration_data(declaration)
                else {
                    return;
                };
                for (field, value) in &entries {
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
                let Some(value) = tomet_semantics::normalized_declaration_args(declaration)
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
    use tomet_type::Type;

    let field_type = field_types.get(field)?;
    let ty = Type::from_name(field_type).filter(|ty| ty.has_text_form())?;
    let well_formed = match value {
        Value::Null => true,
        // `${...}` keeps its place in the tree until it is evaluated,
        // and what it comes to is not known here.
        Value::Interp(_) => true,
        // A scalar is the text it was written as: a bare `1.10` is the
        // version `1.10`, and `2026` is not a datetime.
        scalar => scalar
            .scalar_text()
            .is_some_and(|text| text.is_empty() || ty.holds_text(&text) == Some(true)),
    };
    (!well_formed).then(|| Diagnostic::InvalidFieldValue {
        field: field.to_string(),
        field_type: field_type.clone(),
        value: describe_value(value),
        span,
    })
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
