use std::ops::ControlFlow;

use lsp_types::{GotoDefinitionResponse, Location, Position, Uri};
use tomet_ast::{Element, ElementValue, InterpExprKind, Span, Value};
use tomet_tree::{ElementExt, Visitor, walk_document};

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
                        if k == "id" {
                            if let Value::String(s) = v {
                                self.ref_id = Some(s.clone());
                            }
                        }
                    }
                }
                // Or if it's an interpolation expression ${id}
                if let Some(ElementValue::Interp(expr)) = &el.value {
                    if let InterpExprKind::Identifier(id) = &expr.kind {
                        self.ref_id = Some(id.clone());
                    }
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

fn find_def_in_blocks(blocks: &[tomet_ast::Block], target_id: &str) -> Option<Span> {
    for block in blocks {
        match block {
            tomet_ast::Block::Section(sec) => {
                if let Some(Value::Map(entries)) = section_attrs(sec) {
                    for (k, v) in entries {
                        if k == "id" {
                            if let Value::String(s) = v {
                                if s == target_id {
                                    return Some(sec.span);
                                }
                            }
                        }
                    }
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
            tomet_ast::Block::Element(el) => {
                if let Some(span) = find_def_in_element(el, target_id) {
                    return Some(span);
                }
            }
            tomet_ast::Block::Paragraph(p) => {
                for inline in &p.content {
                    if let tomet_ast::Inline::Element(el) = inline {
                        if let Some(span) = find_def_in_element(el, target_id) {
                            return Some(span);
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_def_in_element(el: &Element, target_id: &str) -> Option<Span> {
    if let Some(Value::Map(entries)) = el.attrs_view() {
        for (k, v) in entries {
            if k == "id" {
                if let Value::String(s) = v {
                    if s == target_id {
                        return Some(el.span);
                    }
                }
            }
        }
    }
    if let Some(content) = &el.content {
        for inline in content {
            if let tomet_ast::Inline::Element(child_el) = inline {
                if let Some(span) = find_def_in_element(child_el, target_id) {
                    return Some(span);
                }
            }
        }
    }
    if let Some(children) = &el.children {
        if let Some(span) = find_def_in_blocks(children, target_id) {
            return Some(span);
        }
    }
    for conn in &el.connects {
        if let Some(span) = find_def_in_element(conn, target_id) {
            return Some(span);
        }
    }
    None
}

fn section_attrs(sec: &tomet_ast::Section) -> Option<Value> {
    match (&sec.args, &sec.value) {
        (Some(args), Some(val)) => match (args, val.as_data()) {
            (Value::Map(m1), Some(Value::Map(m2))) => {
                let mut merged = m1.clone();
                merged.extend(m2);
                Some(Value::Map(merged))
            }
            (_, Some(val)) => Some(val),
            (args, None) => Some(args.clone()),
        },
        (Some(args), None) => Some(args.clone()),
        (None, Some(val)) => val.as_data(),
        (None, None) => None,
    }
}
