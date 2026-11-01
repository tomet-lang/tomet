//! Parsing for `Value`: delegates core value and map parsing to the `cotomet-syntax` crate,
//! while providing a hook for what a Tomet value embeds -- elements (`@element`),
//! `[content]`, `|` runs and dollar expressions (`$interp`) -- which it reads off
//! the CST.

use std::collections::HashMap;

use crate::dialect::Dialect;
use crate::error::{Error, Result};
use tomet_ast::Value;
use tomet_cst::{SyntaxKind as K, SyntaxNode};
use tomet_lexer::Cursor;

pub(crate) use cotomet_syntax::{
    POSITIONAL_ENTRY_KEY, eat_name, eat_scalar_raw, is_name_start_at, skip_inline_ws,
    skip_line_comment, skip_ws_newlines_and_comments,
};

/// Parse an entire source string as one `Value` (a data-only `.tmt`
/// document -- no headings, prose, or top-level elements).
pub fn parse_value(src: &str) -> Result<Value> {
    let root = crate::cst::parse_cst_value(src);
    crate::lower::in_dialect(Dialect::Document, || {
        cotomet_syntax::parse_value_with(src, &mut Embedded::new(src, &root))
    })
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

/// What a value embeds, found in the CST: an element (`@name...`, `?hole(...)`),
/// `[content]`, a `|` run, and the groups of a bare entry in a `{value}`.
///
/// The value grammar (maps, calls, scalars: `cotomet-syntax`) reads the text of a
/// group, and wherever a value or an entry begins it asks this hook. The hook
/// does not parse what stands there; it finds the CST node that starts at that
/// offset and lowers it, so a group's shape is read once, by the CST parser.
pub(crate) struct Embedded<'a> {
    src: &'a str,
    nodes: HashMap<usize, SyntaxNode>,
}

impl<'a> Embedded<'a> {
    /// The embedded nodes below `scope`, by the offset they start at. A node
    /// that is lowered as a whole (an element, a `[content]`) hides what it holds,
    /// which its own lowering finds; a group (`( )`, `{ }`, `#( )`) does not,
    /// because a call's arguments can hold an element.
    pub(crate) fn new(src: &'a str, scope: &SyntaxNode) -> Self {
        let mut nodes = HashMap::new();
        collect(scope, &mut nodes);
        Embedded { src, nodes }
    }

    pub(crate) fn src(&self) -> &'a str {
        self.src
    }

    pub(crate) fn node_at(&self, at: usize) -> Option<&SyntaxNode> {
        self.nodes.get(&at)
    }
}

fn collect(node: &SyntaxNode, into: &mut HashMap<usize, SyntaxNode>) {
    for child in node.children() {
        let at = usize::from(child.text_range().start());
        match child.kind() {
            // `${...}` is an inline element of the CST too, but it is no element:
            // the interpolation grammar reads it.
            K::INLINE_ELEMENT | K::BLOCK_ELEMENT if !opens_with_element_sigil(&child) => {
                collect(&child, into);
            }
            K::INLINE_ELEMENT | K::BLOCK_ELEMENT | K::CONTENT => {
                into.entry(at).or_insert(child);
            }
            K::ARGS | K::VALUE_DATA | K::MAP_LITERAL | K::ID_GROUP => {
                into.entry(at).or_insert_with(|| child.clone());
                collect(&child, into);
            }
            _ => collect(&child, into),
        }
    }
}

/// Whether `node` is written with `@`, or is a hole (`?` or a box), and not `$`.
fn opens_with_element_sigil(node: &SyntaxNode) -> bool {
    crate::lower::slot_of(node).is_some()
        || node
            .children()
            .find(|n| n.kind() == K::SIGIL)
            .and_then(|n| n.first_token())
            .is_some_and(|t| matches!(t.kind(), K::AT | K::QUESTION))
}

fn hooked(e: Error) -> cotomet_syntax::Error {
    cotomet_syntax::Error::at(e.message, e.line, e.column, e.offset)
}

impl cotomet_syntax::ValueHook for Embedded<'_> {
    fn try_parse_value(&mut self, cur: &mut Cursor) -> Option<cotomet_syntax::Result<Value>> {
        if let Some(node) = self.nodes.get(&cur.pos())
            && matches!(
                node.kind(),
                K::INLINE_ELEMENT | K::BLOCK_ELEMENT | K::CONTENT
            )
        {
            return Some(match crate::lower::lower_embedded(self.src, node) {
                Ok((value, end)) => {
                    cur.set_pos(end);
                    Ok(value)
                }
                Err(e) => Err(hooked(e)),
            });
        }
        // `$name(args)`/`${...}` sitting where a value goes.
        CotometValueHook.try_parse_value(cur)
    }
}
