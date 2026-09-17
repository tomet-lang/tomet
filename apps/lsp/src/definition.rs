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
    struct DefFinder {
        target_id: String,
        found_span: Option<Span>,
    }

    impl Visitor<()> for DefFinder {
        fn visit(&mut self, el: &Element) -> ControlFlow<()> {
            if let Some(Value::Map(entries)) = el.attrs_view() {
                for (k, v) in entries {
                    if k == "id" {
                        if let Value::String(s) = v {
                            if s == self.target_id {
                                self.found_span = Some(el.span);
                                return ControlFlow::Break(());
                            }
                        }
                    }
                }
            }
            ControlFlow::Continue(())
        }
    }

    let mut def_finder = DefFinder {
        target_id,
        found_span: None,
    };
    let _ = walk_document(&doc, &mut def_finder);

    def_finder.found_span.map(|span| {
        GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: span_to_range(&span),
        })
    })
}
