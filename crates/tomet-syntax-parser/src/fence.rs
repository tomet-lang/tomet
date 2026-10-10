//! The line-oriented fence scan shared by ``` ``` `` code blocks.
//!
//! A backtick fence closes on a line whose leading run of the same fence
//! character is at least as long as the opening run -- a longer opening
//! run (` ```` `) escapes a body that itself contains a closing-looking
//! line. This is the one piece of that machinery still needed:
//! `raw.rs`'s fenced code block is the only fence left in the
//! grammar.

use crate::error::Result;
use crate::value::skip_inline_ws;
use tomet_lexer::Cursor;

/// Scans forward from `cur` to the closing fence line, returning the
/// decoded `code` string and the `(start, end)` byte range of the body,
/// leaving `cur` just past the closing line.
///
/// A closing line is one whose leading run of `fence_char` is at least
/// `fence_len` long and which holds nothing else but inline whitespace.
/// When `pipe_stack` is non-empty, each line consumes its active pipe markers
/// first, stripping an optional single trailing space delimiter.
pub(crate) fn scan_fenced_body(
    cur: &mut Cursor,
    fence_char: char,
    fence_len: usize,
    pipe_stack: &[usize],
) -> Result<(String, usize, usize)> {
    let body_start = cur.pos();
    if pipe_stack.is_empty() {
        loop {
            if cur.is_eof() {
                let body_end = cur.pos();
                let mut code = cur.src()[body_start..body_end].to_string();
                if code.ends_with('\n') {
                    code.pop();
                }
                return Ok((code, body_start, body_end));
            }
            let line_start = cur.pos();
            let mut look = *cur;
            let run = look.eat_while(|c| c == fence_char).len();
            if run >= fence_len {
                skip_inline_ws(&mut look);
                if matches!(look.peek(), None | Some('\n') | Some('\r')) {
                    cur.set_pos(look.pos());
                    let mut code = cur.src()[body_start..line_start].to_string();
                    if code.ends_with('\n') {
                        code.pop();
                    }
                    return Ok((code, body_start, line_start));
                }
            }
            cur.eat_while(|c| c != '\n' && c != '\r');
            if matches!(cur.peek(), Some('\n') | Some('\r')) {
                cur.bump();
            }
        }
    }

    let mut code = String::new();
    loop {
        if cur.is_eof() {
            break;
        }
        let mut probe = *cur;
        let mut ok = true;
        for &expected_col in pipe_stack {
            skip_inline_ws(&mut probe);
            if probe.peek() != Some('|') {
                ok = false;
                break;
            }
            let (_, marker_col) = probe.line_col(probe.pos());
            if marker_col != expected_col {
                return Err(crate::value::err(
                    &probe,
                    probe.pos(),
                    format!(
                        "this `|` stands in column {marker_col}, and the content it would \
                         continue was opened in column {expected_col}"
                    ),
                ));
            }
            probe.bump();
        }
        if !ok {
            break;
        }

        let mut fence_probe = probe;
        skip_inline_ws(&mut fence_probe);
        let run = fence_probe.eat_while(|c| c == fence_char).len();
        if run >= fence_len {
            skip_inline_ws(&mut fence_probe);
            if matches!(fence_probe.peek(), None | Some('\n') | Some('\r')) {
                cur.set_pos(fence_probe.pos());
                break;
            }
        }

        if probe.peek() == Some(' ') {
            probe.bump();
        }
        let content_start = probe.pos();
        probe.eat_while(|c| c != '\n' && c != '\r');
        code.push_str(probe.slice_from(content_start));
        if matches!(probe.peek(), Some('\n') | Some('\r')) {
            code.push('\n');
            probe.bump();
        }
        cur.set_pos(probe.pos());
    }

    if code.ends_with('\n') {
        code.pop();
    }
    let body_end = cur.pos();
    Ok((code, body_start, body_end))
}
