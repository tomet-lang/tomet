//! Parsing for `^(...)` and `^name(...)` reference pins.

use crate::element::parse_groups;
use crate::error::Result;
use crate::value::{eat_name, err, is_name_start_at};
use tomet_ast::{Element, Placement, Sigil};
use tomet_lexer::Cursor;
use tomet_tree::{ElementExt, element_new};

/// `^` immediately followed by `(` or an identifier and `(`
pub(crate) fn is_caret_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    if look.bump() != Some('^') {
        return false;
    }
    // ^(...)
    if look.peek() == Some('(') {
        return true;
    }
    // ^name(...)
    if is_name_start_at(&look) && eat_name(&mut look).is_some() {
        if look.peek() == Some('(') {
            return true;
        }
    }
    false
}

/// Parses a Caret reference element (`^(...)` or `^name(...)`).
pub(crate) fn parse_caret_element(cur: &mut Cursor, allow_colon_connect: bool) -> Result<Element> {
    let start_pos = cur.pos();
    cur.bump(); // eat '^'
    let target_kind = if cur.peek() == Some('(') {
        None
    } else {
        match eat_name(cur) {
            Some(name) => Some(name),
            None => {
                return Err(err(
                    cur,
                    cur.pos(),
                    "expected element name or '(' after '^'",
                ));
            }
        }
    };
    if cur.peek() != Some('(') {
        return Err(err(cur, cur.pos(), "expected '(' after '^' or '^name'"));
    }
    let sigil = Sigil::Caret(target_kind);
    let mut el = element_new(sigil).with_placement(Placement::Inline);
    parse_groups(cur, &mut el, allow_colon_connect)?;
    el.span = cur.span_from(start_pos);
    Ok(el)
}
