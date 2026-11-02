//! Lowering for `Value` and `File`: folds CST nodes into AST [`Value`] or [`File`].

use crate::error::{Error, Result};
use tomet_ast::{File, Value};
use tomet_cst::SyntaxNode;

/// Lowers a CST root from `parse_cst_value` into an AST [`Value`].
pub fn lower_value(root: &SyntaxNode) -> Result<Value> {
    crate::lower::reject_too_deep(root)?;
    crate::element::lower_value(root)
}

/// Lowers a CST root from `parse_cst_value` into a [`File`].
pub fn lower_file(root: &SyntaxNode, src: &str) -> Result<File> {
    crate::lower::reject_too_deep(root).map_err(|e| e.located_in(src))?;
    let body = crate::element::lower_file_body(root).map_err(|e| e.located_in(src))?;
    Ok(File {
        header: Vec::new(),
        body,
    })
}

/// Shifts spans of all interpolations in `value` by `delta`.
pub fn shift_spans(value: &mut Value, delta: usize) {
    match value {
        Value::Interp(interp) => {
            interp.span.start += delta;
            interp.span.end += delta;
        }
        Value::Map(entries) => {
            for (_, v) in entries {
                shift_spans(v, delta);
            }
        }
        Value::Seq(items) => {
            for v in items {
                shift_spans(v, delta);
            }
        }
        Value::Call(_, args) => {
            for v in args {
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
        Value::Map(entries) => entries.iter().find_map(|(_, val)| find_interp_in_value(val)),
        Value::Seq(items) => items.iter().find_map(find_interp_in_value),
        Value::Call(_, args) => args.iter().find_map(find_interp_in_value),
        _ => None,
    }
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
