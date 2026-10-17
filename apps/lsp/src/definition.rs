use std::ops::ControlFlow;

use lsp_types::{GotoDefinitionResponse, Location, Position, Uri};
use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, InterpExpr, InterpExprKind, List, Span, Value,
};
use tomet_semantics::TargetScheme;
use tomet_tree::{Visitor, walk_document};

use crate::position::{TextPositions, span_contains};

/// What a reference under the cursor refers to.
#[derive(Debug, Clone)]
pub(crate) enum Reference {
    /// A node of this document, by its id.
    Id(String),
    /// Another document, or a node of it.
    In {
        document: Target,
        id: Option<String>,
    },
}

/// How a reference names another document.
#[derive(Debug, Clone)]
pub(crate) enum Target {
    /// `tmt:path`: a path, as written.
    Path(String),
    /// `ref:name`: a note, by its name.
    Note(String),
    /// An expression that comes to a document: `tmt("p")`, `ref("n")`.
    Expression(InterpExpr),
}

/// Provides Goto Definition target for reference links or interpolation
/// expressions, from the text alone: the references to a node of the same
/// document. One into another document needs the vault
/// ([`Workspace::definition`](crate::Workspace::definition)).
pub fn definition_for(text: &str, pos: Position, uri: &Uri) -> Option<GotoDefinitionResponse> {
    let doc = tomet_parser::parse_document(text).ok()?;
    let positions = TextPositions::new(text);
    let Some(Reference::Id(target_id)) = reference_at(&doc, positions.offset(pos)) else {
        return None;
    };

    // Find the node that defines this id
    let found_span = find_def_in_blocks(&doc.blocks, &target_id);

    found_span.map(|span| {
        GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: positions.range(&span),
        })
    })
}

/// The reference the cursor is on, if it is on one.
pub(crate) fn reference_at(doc: &Document, offset: usize) -> Option<Reference> {
    struct Finder {
        offset: usize,
        found: Option<Reference>,
    }

    impl Visitor<()> for Finder {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            // Elements are visited outside in, so the innermost one that
            // refers to something wins.
            if span_contains(&el.span, self.offset)
                && let Some(reference) = reference_in(el, self.offset)
            {
                self.found = Some(reference);
            }
            ControlFlow::Continue(())
        }
    }

    let mut finder = Finder {
        offset,
        found: None,
    };
    let _ = walk_document(doc, &mut finder);
    finder.found
}

/// What `el` refers to.
///
/// - `@link(#x)` and `@link(id:x)`: an anchor in the same document.
/// - `@link(tmt:path)`, `@link(tmt:path#x)`, `@link(ref:Name)`: another
///   document, or a node of it.
/// - an `id:` argument, as in `@deck.ref(id: x)`: a node of this document.
/// - in an expression, `#x` and `self#x` (this document), `doc#x` (a node of
///   another), and `tmt("p")`, `ref("n")` (the document itself). Where the
///   cursor is on more than one, the narrowest; where it is on none, the first.
fn reference_in(el: &Element, offset: usize) -> Option<Reference> {
    if let Some((_, target)) = tomet_semantics::link_target_of(el) {
        let (scheme, rest) = tomet_semantics::target_scheme(&target);
        match scheme {
            TargetScheme::Internal => return Some(Reference::Id(rest.to_string())),
            TargetScheme::Document => {
                let (path, id) = split_anchor(rest);
                return Some(Reference::In {
                    document: Target::Path(path.to_string()),
                    id,
                });
            }
            TargetScheme::Ref => {
                let (name, id) = split_anchor(rest);
                return Some(Reference::In {
                    document: Target::Note(name.trim().to_string()),
                    id,
                });
            }
            _ => {}
        }
    }
    if let Some(Value::Map(entries)) = &el.args {
        for (k, v) in entries {
            if k == "id"
                && let Value::String(s) = v
            {
                return Some(Reference::Id(s.clone()));
            }
        }
    }
    if let Some(ElementValue::Interp(expr)) = &el.value {
        let mut found = Vec::new();
        references_in(expr, &mut found);
        let narrowest = found
            .iter()
            .filter(|(span, _)| span_contains(span, offset))
            .min_by_key(|(span, _)| span.end.saturating_sub(span.start));
        return narrowest
            .or_else(|| found.first())
            .map(|(_, reference)| reference.clone());
    }
    None
}

/// `path#x` as the path and the id, if there is one.
fn split_anchor(rest: &str) -> (&str, Option<String>) {
    match rest.split_once('#') {
        Some((path, id)) if !id.trim().is_empty() => (path.trim(), Some(id.trim().to_string())),
        Some((path, _)) => (path.trim(), None),
        None => (rest.trim(), None),
    }
}

/// Every reference in `expr`, with where it is written.
fn references_in(expr: &InterpExpr, out: &mut Vec<(Span, Reference)>) {
    match &expr.kind {
        InterpExprKind::Id(id) => out.push((expr.span, Reference::Id(id.clone()))),
        InterpExprKind::DocId { doc, id } => {
            if matches!(&doc.kind, InterpExprKind::Identifier(name) if name == "self") {
                out.push((expr.span, Reference::Id(id.clone())));
            } else {
                out.push((
                    expr.span,
                    Reference::In {
                        document: Target::Expression((**doc).clone()),
                        id: Some(id.clone()),
                    },
                ));
                references_in(doc, out);
            }
        }
        InterpExprKind::Call { callee, args } => {
            if matches!(&callee.kind, InterpExprKind::Identifier(name) if name == "tmt" || name == "ref")
            {
                out.push((
                    expr.span,
                    Reference::In {
                        document: Target::Expression(expr.clone()),
                        id: None,
                    },
                ));
            } else {
                references_in(callee, out);
            }
            for arg in args {
                references_in(arg, out);
            }
        }
        InterpExprKind::Member { object, .. } => references_in(object, out),
        InterpExprKind::NamedArg { value, .. } => references_in(value, out),
        InterpExprKind::Binary { lhs, rhs, .. } => {
            references_in(lhs, out);
            references_in(rhs, out);
        }
        InterpExprKind::Group(inner) => references_in(inner, out),
        InterpExprKind::Lambda { body, .. } => references_in(body, out),
        InterpExprKind::Identifier(_)
        | InterpExprKind::Literal(_)
        | InterpExprKind::Addr { .. } => {}
    }
}

pub(crate) fn find_def_in_blocks(blocks: &[Block], target_id: &str) -> Option<Span> {
    for block in blocks {
        match block {
            Block::Section(sec) => {
                if sec.id.as_ref().is_some_and(|id| id.0 == target_id) {
                    return Some(sec.span);
                }
                for conn in &sec.connects {
                    if let Some(span) = find_def_in_element(conn, target_id) {
                        return Some(span);
                    }
                }
                if let Some(span) = find_def_in_blocks(&sec.blocks, target_id) {
                    return Some(span);
                }
            }
            Block::Element(el) => {
                if let Some(span) = find_def_in_element(el, target_id) {
                    return Some(span);
                }
            }
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline
                        && let Some(span) = find_def_in_element(el, target_id)
                    {
                        return Some(span);
                    }
                }
            }
            Block::List(list) => {
                if let Some(span) = find_def_in_list(list, target_id) {
                    return Some(span);
                }
            }
        }
    }
    None
}

fn find_def_in_list(list: &List, target_id: &str) -> Option<Span> {
    if list.id.as_ref().is_some_and(|id| id.0 == target_id) {
        return Some(list.span);
    }
    for conn in &list.connects {
        if let Some(span) = find_def_in_element(conn, target_id) {
            return Some(span);
        }
    }
    for item in &list.items {
        if let Some(span) = find_def_in_element(&item.element, target_id) {
            return Some(span);
        }
        if let Some(sub) = &item.sublist
            && let Some(span) = find_def_in_list(sub, target_id)
        {
            return Some(span);
        }
    }
    None
}

fn find_def_in_element(el: &Element, target_id: &str) -> Option<Span> {
    if el.id.as_ref().is_some_and(|id| id.0 == target_id) {
        return Some(el.span);
    }
    if let Some(content) = &el.content
        && let Some(span) = find_def_in_blocks(content, target_id)
    {
        return Some(span);
    }
    for conn in &el.connects {
        if let Some(span) = find_def_in_element(conn, target_id) {
            return Some(span);
        }
    }
    None
}
