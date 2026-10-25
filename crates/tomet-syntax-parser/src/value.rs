//! Parsing for `Value`: delegates core value and map parsing to the `cotomet-syntax` crate,
//! while providing custom hooks for embedded elements (`@element`) and
//! dollar expressions (`$interp`) within Tomet documents.

use crate::dialect::Dialect;
use crate::error::{Error, Result};
use tomet_ast::Value;
use tomet_lexer::Cursor;

pub(crate) use cotomet_syntax::{
    POSITIONAL_ENTRY_KEY, eat_name, eat_scalar_raw, is_name_start_at, skip_inline_ws,
    skip_line_comment, skip_ws_and_newlines, skip_ws_newlines_and_comments,
};

/// Parse an entire source string as one `Value` (a data-only `.tmt`
/// document -- no headings, prose, or top-level elements).
pub fn parse_value(src: &str) -> Result<Value> {
    cotomet_syntax::parse_value_with(
        src,
        &mut TometValueHook {
            dialect: Dialect::Document,
        },
    )
    .map_err(Into::into)
}

pub(crate) fn err(pos: usize, message: impl Into<String>) -> Error {
    Error {
        message: message.into(),
        line: 0,
        column: 0,
        offset: pos,
    }
}

pub(crate) fn skip_block_comment(cur: &mut Cursor) -> Result<()> {
    cotomet_syntax::skip_block_comment(cur).map_err(Into::into)
}

pub(crate) fn parse_quoted(cur: &mut Cursor) -> Result<String> {
    cotomet_syntax::parse_quoted(cur).map_err(Into::into)
}

/// A [`cotomet_syntax::ValueHook`] allowing only dollar expressions (`${...}`, `$name(...)`)
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
            let el = match crate::interp::parse_dollar_element(cur) {
                Ok(el) => el,
                Err(e) => {
                    return Some(Err(cotomet_syntax::Error::at(
                        e.message, e.line, e.column, e.offset,
                    )));
                }
            };
            return Some(Ok(Value::Element(Box::new(el))));
        }

        None
    }
}

/// A [`cotomet_syntax::ValueHook`] allowing embedded elements (`@...`) and dollar expressions (`$...`)
/// to be parsed inside value positions within Tomet.
pub(crate) struct TometValueHook {
    pub(crate) dialect: Dialect,
}

impl cotomet_syntax::ValueHook for TometValueHook {
    fn try_parse_value(&mut self, cur: &mut Cursor) -> Option<cotomet_syntax::Result<Value>> {
        // A real, `@`-sigiled element sitting in a value slot
        if self.dialect.opens_element(cur.peek())
            && crate::element::is_element_start(self.dialect, cur, false)
        {
            let el = match crate::element::parse_element(self.dialect, cur, false) {
                Ok(el) => el,
                Err(e) => {
                    return Some(Err(cotomet_syntax::Error::at(
                        e.message, e.line, e.column, e.offset,
                    )));
                }
            };
            return Some(Ok(Value::Element(Box::new(el))));
        }

        // Delegate dollar expressions (`$name(args)`/`${...}`) to CotometValueHook.
        if let Some(res) = CotometValueHook.try_parse_value(cur) {
            return Some(res);
        }

        // `key: [ ... ]` -- block content as a value (`Value::Blocks`),
        // the same recursive grammar `[content]` uses, bounded by `]`
        // instead of consuming an enclosing element's own closer. Without
        // this hook, `cotomet`'s own value grammar rejects `[` outright
        // ("'[...]' list literal was removed").
        if cur.peek() == Some('[') {
            cur.bump();
            let blocks = match crate::document::parse_block_seq(
                self.dialect,
                cur,
                crate::document::BlockStop::Bracket(']'),
                &[],
            ) {
                Ok(blocks) => blocks,
                Err(e) => {
                    return Some(Err(cotomet_syntax::Error::at(
                        e.message, e.line, e.column, e.offset,
                    )));
                }
            };
            if !cur.eat_str("]") {
                return Some(Err(cotomet_syntax::Error::at_offset(
                    "expected ']'",
                    cur.pos(),
                )));
            }
            return Some(Ok(Value::Blocks(blocks)));
        }

        // `key: | prose` -- the `|content` run as a value (`Value::Blocks`),
        // the same grammar an element's trailing `| prose` uses. The run
        // ends where the marker column stops continuing it, which is what
        // keeps a `,` or `}` inside the prose from closing the entry.
        if cur.peek() == Some('|') {
            return Some(
                crate::element::parse_pipe_content(self.dialect, cur, &[])
                    .map(Value::Blocks)
                    .map_err(|e| cotomet_syntax::Error::at(e.message, e.line, e.column, e.offset)),
            );
        }

        None
    }
}

pub(crate) fn parse_value_at(dialect: Dialect, cur: &mut Cursor) -> Result<Value> {
    cotomet_syntax::parse_value_at_with(cur, &mut TometValueHook { dialect }).map_err(Into::into)
}

pub(crate) fn parse_one_entry(dialect: Dialect, cur: &mut Cursor) -> Result<(String, Value)> {
    cotomet_syntax::parse_one_entry_with(cur, &mut TometValueHook { dialect }).map_err(Into::into)
}
