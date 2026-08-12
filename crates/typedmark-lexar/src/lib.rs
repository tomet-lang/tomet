//! A minimal byte-position cursor over `&str` source text.
//!
//! TypedMark's grammar is context-sensitive -- `<`, `@`, `(`, `[`, `{` are
//! only structural right after specific lookaheads (an element trigger),
//! and plain prose text otherwise. That's easier to scan directly with a
//! cursor than to pre-tokenize into a context-free stream, so
//! `typedmark-parser` builds its recursive-descent parser straight on top
//! of this rather than going through a separate token pass.

use typedmark_ast::{Position, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(src: &'a str) -> Self {
        Cursor { src, pos: 0 }
    }

    pub fn src(&self) -> &'a str {
        self.src
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        debug_assert!(self.src.is_char_boundary(pos));
        self.pos = pos;
    }

    pub fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    pub fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    pub fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub fn peek_at(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    pub fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    /// Advance past one character, returning it.
    pub fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Advance past `s` if the cursor is currently positioned at it.
    pub fn eat_str(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    /// Advance while `pred` holds, returning the consumed slice.
    pub fn eat_while(&mut self, mut pred: impl FnMut(char) -> bool) -> &'a str {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if !pred(c) {
                break;
            }
            self.bump();
        }
        &self.src[start..self.pos]
    }

    /// The slice from `start` (a previously recorded position) to here.
    pub fn slice_from(&self, start: usize) -> &'a str {
        &self.src[start..self.pos]
    }

    /// 1-based line/column of a byte position, for error reporting.
    pub fn line_col(&self, pos: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for c in self.src[..pos].chars() {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Returns a [`Position`] for a given byte offset.
    pub fn position_at(&self, pos: usize) -> Position {
        let (line, column) = self.line_col(pos);
        Position::new(line, column, pos)
    }

    /// Returns the current [`Position`] of the cursor.
    pub fn current_position(&self) -> Position {
        self.position_at(self.pos)
    }

    /// Returns a [`Span`] from `start_pos` byte offset to the current cursor position.
    pub fn span_from(&self, start_pos: usize) -> Span {
        Span::new(self.position_at(start_pos), self.current_position())
    }
}
