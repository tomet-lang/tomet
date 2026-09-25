//! Parsing for `Value`: delegates core value and map parsing to the `tove` crate,
//! while providing custom hooks for embedded elements (`@element`) and
//! dollar expressions (`$interp`) within Tomet documents.

use crate::error::{Error, Result};
use tomet_ast::Value;
use tomet_lexer::Cursor;

pub(crate) use tove::{
    POSITIONAL_ENTRY_KEY, eat_name, is_name_start_at, skip_inline_ws, skip_line_comment,
    skip_ws_and_newlines, skip_ws_newlines_and_comments,
};

/// Parse an entire source string as one `Value` (a data-only `.tmt`
/// document -- no headings, prose, or top-level elements).
pub fn parse_value(src: &str) -> Result<Value> {
    tove::parse_value_with(src, &mut TometValueHook).map_err(Into::into)
}

pub(crate) fn err(cur: &Cursor, pos: usize, message: impl Into<String>) -> Error {
    let (line, column) = cur.line_col(pos);
    Error {
        message: message.into(),
        line,
        column,
        offset: pos,
    }
}

pub(crate) fn skip_block_comment(cur: &mut Cursor) -> Result<()> {
    tove::skip_block_comment(cur).map_err(Into::into)
}

pub(crate) fn parse_quoted(cur: &mut Cursor) -> Result<String> {
    tove::parse_quoted(cur).map_err(Into::into)
}

/// A [`tove::ValueHook`] allowing embedded elements (`@...`) and dollar expressions (`$...`)
/// to be parsed inside value positions within Tomet.
pub(crate) struct TometValueHook;

impl tove::ValueHook for TometValueHook {
    fn try_parse_value(&mut self, cur: &mut Cursor) -> Option<tove::Result<Value>> {
        // A real, `@`-sigiled element sitting in a value slot
        if cur.peek() == Some('@') && crate::element::is_element_start(cur, false) {
            let start = cur.pos();
            let el = match crate::element::parse_element(cur, false) {
                Ok(el) => el,
                Err(e) => return Some(Err(tove::Error::at(e.message, e.line, e.column, e.offset))),
            };
            // An element embedded in a value position cannot carry block children.
            if el.children.is_some() {
                let (line, col) = cur.line_col(start);
                return Some(Err(tove::Error::at(
                    "an element embedded in a value may not take block children",
                    line,
                    col,
                    start,
                )));
            }
            return Some(Ok(Value::Element(Box::new(el))));
        }

        // `$name(args)`/`${...}` sitting where a value goes -- evaluated later by `tomet-transform::interp`.
        if cur.peek() == Some('$') && crate::interp::is_interp_start(cur) {
            let el = match crate::interp::parse_dollar_element(cur) {
                Ok(el) => el,
                Err(e) => return Some(Err(tove::Error::at(e.message, e.line, e.column, e.offset))),
            };
            return Some(Ok(Value::Element(Box::new(el))));
        }

        None
    }
}

pub(crate) fn parse_value_at(cur: &mut Cursor) -> Result<Value> {
    tove::parse_value_at_with(cur, &mut TometValueHook).map_err(Into::into)
}

pub(crate) fn parse_one_entry(cur: &mut Cursor) -> Result<(String, Value)> {
    tove::parse_one_entry_with(cur, &mut TometValueHook).map_err(Into::into)
}
