//! The line-oriented fence scan shared by ``` ``` `` code blocks.
//!
//! A backtick fence closes on a line whose leading run of the same fence
//! character is at least as long as the opening run -- a longer opening
//! run (` ```` `) escapes a body that itself contains a closing-looking
//! line. This is the one piece of that machinery still needed:
//! `codeblock.rs`'s fenced code block is the only fence left in the
//! grammar.

use crate::value::skip_inline_ws;
use tomet_lexer::Cursor;

/// Scans forward from `cur` to the closing fence line, returning the
/// `(start, end)` byte range of the body and leaving `cur` just past the
/// closing line.
///
/// A closing line is one whose leading run of `fence_char` is at least
/// `fence_len` long and which holds nothing else but inline whitespace.
/// Reaching EOF first ends the body there.
pub(crate) fn scan_fenced_body(
    cur: &mut Cursor,
    fence_char: char,
    fence_len: usize,
) -> (usize, usize) {
    let body_start = cur.pos();
    loop {
        if cur.is_eof() {
            return (body_start, cur.pos());
        }
        let line_start = cur.pos();
        let mut look = *cur;
        let run = look.eat_while(|c| c == fence_char).len();
        if run >= fence_len {
            skip_inline_ws(&mut look);
            if matches!(look.peek(), None | Some('\n') | Some('\r')) {
                cur.set_pos(look.pos());
                if matches!(cur.peek(), Some('\n') | Some('\r')) {
                    cur.bump();
                }
                return (body_start, line_start);
            }
        }
        cur.eat_while(|c| c != '\n' && c != '\r');
        if matches!(cur.peek(), Some('\n') | Some('\r')) {
            cur.bump();
        }
    }
}
