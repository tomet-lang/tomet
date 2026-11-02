//! Parsing for `Value`: uses the CST value parser and folds it into an AST [`Value`].

use crate::error::{Error, Result};
use tomet_ast::Value;
use tomet_lexer::Cursor;


/// Parse an entire source string as one `Value` (a data-only `.tmt`
/// document -- no headings, prose, or top-level elements).
pub fn parse_value(src: &str) -> Result<Value> {
    let root = crate::cst::parse_cst_value(src);
    crate::lower::reject_too_deep(&root).map_err(|e| e.located_in(src))?;
    crate::element::lower_value(&root).map_err(|e| e.located_in(src))
}

pub(crate) fn err(pos: usize, message: impl Into<String>) -> Error {
    Error {
        message: message.into(),
        line: 0,
        column: 0,
        offset: pos,
    }
}

/// A [`cotomet_syntax::ValueHook`] allowing only interpolations (`${...}`, `$name(...)`)
/// to be parsed inside value positions.
///
/// Embedded elements (`@element`), block sequences (`[...]`), and prose pipes (`| prose`)
/// are declined so they are rejected by cotomet grammar.
#[derive(Clone, Copy, Debug, Default)]
pub struct CotometValueHook;

impl cotomet_syntax::ValueHook for CotometValueHook {
    fn try_parse_value(&mut self, cur: &mut Cursor) -> Option<cotomet_syntax::Result<Value>> {
        // `$name(args)`/`${...}` sitting where a value goes -- evaluated later.
        if cur.peek() == Some('$') && cotomet_syntax::is_interp_start(cur) {
            let interp = match crate::interp::parse_interpolation(cur) {
                Ok(interp) => interp,
                Err(e) => {
                    return Some(Err(cotomet_syntax::Error::at(
                        e.message, e.line, e.column, e.offset,
                    )));
                }
            };
            return Some(Ok(Value::Interp(interp)));
        }

        None
    }
}
