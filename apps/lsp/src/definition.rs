use std::ops::ControlFlow;

use lsp_types::{GotoDefinitionResponse, Location, Position, Uri};
use tomet_ast::{Block, Element, ElementValue, Inline, InterpExprKind, List, Span, Value};
use tomet_tree::{Visitor, walk_document};

use crate::position::{span_contains, span_to_range};

/// Provides Goto Definition target for reference links or interpolation expressions.
pub fn definition_for(text: &str, pos: Position, uri: &Uri) -> Option<GotoDefinitionResponse> {
    let doc = tomet_parser::parse_document(text).ok()?;
    let target_line = pos.line as usize + 1;
    let target_col = pos.character as usize + 1;

    // Find the node under cursor to get the referenced ID
    struct TargetIdFinder {
        line: usize,
        col: usize,
        ref_id: Option<String>,
    }

    impl Visitor<()> for TargetIdFinder {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            if span_contains(&el.span, self.line, self.col) {
                // Check whether it carries an `id` arg
                if let Some(Value::Map(entries)) = &el.args {
                    for (k, v) in entries {
                        if k == "id"
                            && let Value::String(s) = v
                        {
                            self.ref_id = Some(s.clone());
                        }
                    }
                }
                // Or if it's an interpolation expression ${id}
                if let Some(ElementValue::Interp(expr)) = &el.value
                    && let InterpExprKind::Identifier(id) = &expr.kind
                {
                    self.ref_id = Some(id.clone());
                }
            }
            ControlFlow::Continue(())
        }
    }

    let mut id_finder = TargetIdFinder {
        line: target_line,
        col: target_col,
        ref_id: None,
    };
    let _ = walk_document(&doc, &mut id_finder);

    let target_id = id_finder.ref_id?;

    // Find the node that defines this id
    let found_span = find_def_in_blocks(&doc.blocks, &target_id);

    found_span.map(|span| {
        GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: span_to_range(&span),
        })
    })
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
