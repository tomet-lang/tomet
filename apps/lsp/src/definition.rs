use std::ops::ControlFlow;

use lsp_types::{GotoDefinitionResponse, Location, Position, Uri};
use tomet_ast::{
    Block, Element, ElementValue, Inline, InterpExpr, InterpExprKind, List, Span, Value,
};
use tomet_tree::{Visitor, walk_document};

use crate::position::{TextPositions, span_contains};

/// Provides Goto Definition target for reference links or interpolation expressions.
pub fn definition_for(text: &str, pos: Position, uri: &Uri) -> Option<GotoDefinitionResponse> {
    let doc = tomet_parser::parse_document(text).ok()?;
    let positions = TextPositions::new(text);
    let offset = positions.offset(pos);

    // Find the node under cursor to get the referenced ID
    struct TargetIdFinder {
        offset: usize,
        ref_id: Option<String>,
    }

    impl Visitor<()> for TargetIdFinder {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            // Elements are visited outside in, so the innermost one that
            // refers to something wins.
            if span_contains(&el.span, self.offset)
                && let Some(id) = referenced_id(el, self.offset)
            {
                self.ref_id = Some(id);
            }
            ControlFlow::Continue(())
        }
    }

    let mut id_finder = TargetIdFinder {
        offset,
        ref_id: None,
    };
    let _ = walk_document(&doc, &mut id_finder);

    let target_id = id_finder.ref_id?;

    // Find the node that defines this id
    let found_span = find_def_in_blocks(&doc.blocks, &target_id);

    found_span.map(|span| {
        GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: positions.range(&span),
        })
    })
}

/// The id of the node in this document that `el` refers to.
///
/// - `@link(#x)` and `@link(id:x)`: a link to an anchor in the same document.
/// - an `id:` argument, as in `@deck.ref(id: x)`.
/// - `${#x}`, and `${self#x}`, anywhere in an expression -- the `#x` the cursor
///   is on, or the first when it is on none of them.
///
/// A reference into another document (`tmt:path#x`, `doc#x`) is not followed.
fn referenced_id(el: &Element, offset: usize) -> Option<String> {
    if let Some((_, target)) = tomet_semantics::link_target_of(el) {
        let (scheme, id) = tomet_semantics::target_scheme(&target);
        if scheme == tomet_semantics::TargetScheme::Internal {
            return Some(id.to_string());
        }
    }
    if let Some(Value::Map(entries)) = &el.args {
        for (k, v) in entries {
            if k == "id"
                && let Value::String(s) = v
            {
                return Some(s.clone());
            }
        }
    }
    if let Some(ElementValue::Interp(expr)) = &el.value {
        let mut found = Vec::new();
        same_document_ids(expr, &mut found);
        return found
            .iter()
            .find(|(span, _)| span_contains(span, offset))
            .or_else(|| found.first())
            .map(|(_, id)| id.clone());
    }
    None
}

/// Every `#x` and `self#x` in `expr`, with where it is written.
fn same_document_ids(expr: &InterpExpr, out: &mut Vec<(Span, String)>) {
    match &expr.kind {
        InterpExprKind::Id(id) => out.push((expr.span, id.clone())),
        InterpExprKind::DocId { doc, id } => {
            if matches!(&doc.kind, InterpExprKind::Identifier(name) if name == "self") {
                out.push((expr.span, id.clone()));
            } else {
                same_document_ids(doc, out);
            }
        }
        InterpExprKind::Call { callee, args } => {
            same_document_ids(callee, out);
            for arg in args {
                same_document_ids(arg, out);
            }
        }
        InterpExprKind::Member { object, .. } => same_document_ids(object, out),
        InterpExprKind::NamedArg { value, .. } => same_document_ids(value, out),
        InterpExprKind::Binary { lhs, rhs, .. } => {
            same_document_ids(lhs, out);
            same_document_ids(rhs, out);
        }
        InterpExprKind::Group(inner) => same_document_ids(inner, out),
        InterpExprKind::Lambda { body, .. } => same_document_ids(body, out),
        InterpExprKind::Identifier(_)
        | InterpExprKind::Literal(_)
        | InterpExprKind::Addr { .. } => {}
    }
}

fn find_def_in_blocks(blocks: &[Block], target_id: &str) -> Option<Span> {
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
