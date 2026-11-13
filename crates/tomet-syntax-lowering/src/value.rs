//! Lowering for `Value` and `File`: folds CST nodes into AST [`Value`] or [`File`].

use crate::error::{Error, Result};
use tomet_ast::{File, Value};
use tomet_cst::SyntaxNode;

/// Lowers a CST root from `parse_cst_value` into an AST [`Value`].
pub fn lower_value(root: &SyntaxNode) -> Result<Value> {
    crate::lower::reject_too_deep(root)?;
    crate::element::lower_value(root)
}

/// Lowers a CST root from `parse_cst_value` into a data [`File<Value>`].
pub fn lower_data_file(root: &SyntaxNode, src: &str) -> Result<File<Value>> {
    crate::lower::reject_too_deep(root).map_err(|e| e.located_in(src))?;
    let body = crate::element::lower_file_body(root).map_err(|e| e.located_in(src))?;
    Ok(File::new(
        Vec::new(),
        body,
        tomet_span::Span::new(0, src.len()),
    ))
}

/// Shifts spans of all interpolations and type seats in `value` by `delta`.
pub fn shift_spans(value: &mut Value, delta: usize) {
    match value {
        Value::Interp(interp) => {
            interp.span.start += delta;
            interp.span.end += delta;
        }
        Value::TypeSeat(seat) => {
            seat.span.start += delta;
            seat.span.end += delta;
        }
        Value::Tuple(tuple) | Value::Call(_, tuple) => {
            for v in tuple.values_mut() {
                shift_spans(v, delta);
            }
        }
        _ => {}
    }
}

/// Finds the byte offset of the first interpolation in `v`, if any.
pub fn find_interp_in_value(v: &Value) -> Option<usize> {
    match v {
        Value::Interp(it) => Some(it.span.start),
        Value::Tuple(tuple) | Value::Call(_, tuple) => find_interp_in_tuple(tuple),
        _ => None,
    }
}

/// Finds the byte offset of the first interpolation in `t`, if any.
pub fn find_interp_in_tuple(t: &tomet_ast::Tuple) -> Option<usize> {
    t.values().find_map(find_interp_in_value)
}

/// Finds the byte offset of the first interpolation in `g`, if any.
pub fn find_interp_in_group(g: &tomet_ast::Group) -> Option<usize> {
    g.0.iter().find_map(|e| match e {
        tomet_ast::Entry::Pair(_, v) => find_interp_in_value(v),
        _ => None,
    })
}

pub(crate) fn err(pos: usize, message: impl Into<String>) -> Error {
    Error {
        message: message.into(),
        line: 0,
        column: 0,
        offset: pos,
    }
}
