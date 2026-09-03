//! The `+++` raw-body fence, and the line-oriented fence scan shared with
//! ``` ``` `` code blocks.
//!
//! A `+++` fence sits at the end of an element head and carries the
//! element's body verbatim:
//!
//! ```text
//! #memo+++
//! don't forget: check [this] and [that]
//! +++
//!
//! @meta(format:yaml)+++
//! key: value
//! +++
//! ```
//!
//! It replaces both `(content:raw)[...]` and `(format:x){...}`. Those two
//! were the places the parser had to consult an element's own arguments to
//! decide how to lex its body; a fence is decided entirely by the
//! delimiter, so the tree no longer depends on any vocabulary.
//!
//! Being line-oriented rather than bracket-matched also removes a whole
//! class of bug. `(content:raw)[...]` counted brackets, so a full-width
//! `｛｝` or an unquoted `}` inside legal YAML could end the body early;
//! a closing line of `+++` cannot be miscounted.
//!
//! `${...}` does **not** expand inside a fence. `default.config.tmt`
//! stores macro templates such as `"https://github.com/.../${1}"` that
//! must reach `tomet-transform`'s `MacroPattern::from_template` verbatim.

use crate::error::Result;
use crate::value::skip_inline_ws;
use tomet_lexer::Cursor;

/// The shortest legal fence run.
const MIN_FENCE: usize = 3;

/// Whether `cur` sits at the opening of a `+++` fence.
pub(crate) fn is_fence_start(cur: &Cursor) -> bool {
    let mut look = *cur;
    look.eat_while(|c| c == '+').len() >= MIN_FENCE
}

/// Parses a `+++` fence body, returning it verbatim.
///
/// `cur` must be positioned at the opening run. A longer opening run
/// (`++++`) escapes a body that itself contains a `+++` line, matching the
/// backtick fence's rule. An unterminated fence runs to EOF without a
/// diagnostic -- again matching the backtick fence, and unlike `[...]` /
/// `{...}` groups, which error when unclosed.
pub(crate) fn parse_fence(cur: &mut Cursor) -> Result<String> {
    let fence_len = cur.eat_while(|c| c == '+').len();
    debug_assert!(fence_len >= MIN_FENCE);

    // Anything trailing the opening run on its own line is ignored, so a
    // stray space after `+++` doesn't become part of the body.
    skip_inline_ws(cur);
    cur.eat_while(|c| c != '\n' && c != '\r');
    if matches!(cur.peek(), Some('\n') | Some('\r')) {
        cur.bump();
    }

    let (body_start, body_end) = scan_fenced_body(cur, '+', fence_len);
    let mut body = cur.src()[body_start..body_end].to_string();
    if body.ends_with('\n') {
        body.pop();
    }
    if body.ends_with('\r') {
        body.pop();
    }
    Ok(body)
}

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
