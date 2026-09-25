//! A minimal byte-position cursor over `&str` source text.
//!
//! Tomet's grammar is context-sensitive -- `<`, `@`, `(`, `[`, `{` are
//! only structural right after specific lookaheads (an element trigger),
//! and plain prose text otherwise. That's easier to scan directly with a
//! cursor than to pre-tokenize into a context-free stream, so
//! `tomet-parser` builds its recursive-descent parser straight on top
//! of this rather than going through a separate token pass.

use tomet_ast::{Position, Span};

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

    /// Advance past `expected` if the cursor is currently positioned at it.
    pub fn eat_char(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Advance past the current character if `pred` holds for it, returning it.
    pub fn eat_if(&mut self, pred: impl FnOnce(char) -> bool) -> Option<char> {
        if let Some(c) = self.peek()
            && pred(c)
        {
            self.bump();
            return Some(c);
        }
        None
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

    /// Advance past all whitespace characters, returning the consumed slice.
    pub fn eat_whitespace(&mut self) -> &'a str {
        self.eat_while(|c| c.is_whitespace())
    }

    /// Advance until `target` character or EOF is encountered, returning the consumed slice.
    pub fn eat_until_char(&mut self, target: char) -> &'a str {
        self.eat_while(|c| c != target)
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

pub use tomet_cst::SyntaxKind;

/// Losslessly tokenizes `src` into a list of `(SyntaxKind, &str)` tokens.
///
/// Guarantee: Concatenating all returned token slices produces the exact original source string.
pub fn tokenize(src: &str) -> Vec<(SyntaxKind, &str)> {
    let mut cursor = Cursor::new(src);
    let mut tokens = Vec::new();

    while !cursor.is_eof() {
        let start = cursor.pos();
        let c = cursor.peek().unwrap();

        match c {
            '\n' => {
                cursor.bump();
                tokens.push((SyntaxKind::NEWLINE, cursor.slice_from(start)));
            }
            ' ' | '\t' | '\r' => {
                cursor.eat_while(|ch| ch == ' ' || ch == '\t' || ch == '\r');
                tokens.push((SyntaxKind::WHITESPACE, cursor.slice_from(start)));
            }
            '/' if cursor.starts_with("//") => {
                cursor.eat_while(|ch| ch != '\n');
                tokens.push((SyntaxKind::COMMENT, cursor.slice_from(start)));
            }
            '@' => {
                cursor.bump();
                tokens.push((SyntaxKind::AT, cursor.slice_from(start)));
            }
            '#' => {
                cursor.bump();
                tokens.push((SyntaxKind::HASH, cursor.slice_from(start)));
            }
            '<' => {
                cursor.bump();
                tokens.push((SyntaxKind::LT, cursor.slice_from(start)));
            }
            '>' => {
                cursor.bump();
                tokens.push((SyntaxKind::GT, cursor.slice_from(start)));
            }
            '(' => {
                cursor.bump();
                tokens.push((SyntaxKind::L_PAREN, cursor.slice_from(start)));
            }
            ')' => {
                cursor.bump();
                tokens.push((SyntaxKind::R_PAREN, cursor.slice_from(start)));
            }
            '[' => {
                cursor.bump();
                tokens.push((SyntaxKind::L_BRACKET, cursor.slice_from(start)));
            }
            ']' => {
                cursor.bump();
                tokens.push((SyntaxKind::R_BRACKET, cursor.slice_from(start)));
            }
            '{' => {
                cursor.bump();
                tokens.push((SyntaxKind::L_BRACE, cursor.slice_from(start)));
            }
            '}' => {
                cursor.bump();
                tokens.push((SyntaxKind::R_BRACE, cursor.slice_from(start)));
            }
            ':' => {
                cursor.bump();
                tokens.push((SyntaxKind::COLON, cursor.slice_from(start)));
            }
            ',' => {
                cursor.bump();
                tokens.push((SyntaxKind::COMMA, cursor.slice_from(start)));
            }
            '=' => {
                cursor.bump();
                tokens.push((SyntaxKind::EQUAL, cursor.slice_from(start)));
            }
            '-' => {
                cursor.bump();
                tokens.push((SyntaxKind::MINUS, cursor.slice_from(start)));
            }
            '+' => {
                cursor.bump();
                tokens.push((SyntaxKind::PLUS, cursor.slice_from(start)));
            }
            '*' => {
                cursor.bump();
                tokens.push((SyntaxKind::STAR, cursor.slice_from(start)));
            }
            '/' => {
                cursor.bump();
                tokens.push((SyntaxKind::SLASH, cursor.slice_from(start)));
            }
            '$' => {
                cursor.bump();
                tokens.push((SyntaxKind::DOLLAR, cursor.slice_from(start)));
            }
            '%' => {
                cursor.bump();
                tokens.push((SyntaxKind::PERCENT, cursor.slice_from(start)));
            }
            '|' => {
                cursor.bump();
                tokens.push((SyntaxKind::PIPE, cursor.slice_from(start)));
            }
            '`' => {
                cursor.bump();
                tokens.push((SyntaxKind::BACKTICK, cursor.slice_from(start)));
            }
            '.' => {
                cursor.bump();
                tokens.push((SyntaxKind::DOT, cursor.slice_from(start)));
            }
            '!' => {
                cursor.bump();
                tokens.push((SyntaxKind::EXCLAMATION, cursor.slice_from(start)));
            }
            '?' => {
                cursor.bump();
                tokens.push((SyntaxKind::QUESTION, cursor.slice_from(start)));
            }
            '&' => {
                cursor.bump();
                tokens.push((SyntaxKind::AMPERSAND, cursor.slice_from(start)));
            }
            '~' => {
                cursor.bump();
                tokens.push((SyntaxKind::TILDE, cursor.slice_from(start)));
            }
            '^' => {
                cursor.bump();
                tokens.push((SyntaxKind::CARET, cursor.slice_from(start)));
            }
            ';' => {
                cursor.bump();
                tokens.push((SyntaxKind::SEMI, cursor.slice_from(start)));
            }
            '"' | '\'' => {
                let quote = c;
                cursor.bump();
                let mut escaped = false;
                while let Some(ch) = cursor.peek() {
                    cursor.bump();
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == quote {
                        break;
                    }
                }
                tokens.push((SyntaxKind::STRING_LITERAL, cursor.slice_from(start)));
            }
            '0'..='9' => {
                cursor.eat_while(|ch| ch.is_ascii_digit());
                if cursor.peek() == Some('.')
                    && cursor.peek_at(1).is_some_and(|ch| ch.is_ascii_digit())
                {
                    cursor.bump(); // consume '.'
                    cursor.eat_while(|ch| ch.is_ascii_digit());
                    tokens.push((SyntaxKind::FLOAT_NUMBER, cursor.slice_from(start)));
                } else {
                    tokens.push((SyntaxKind::INT_NUMBER, cursor.slice_from(start)));
                }
            }
            _ if is_ident_start(c) => {
                cursor.eat_while(is_ident_continue);
                let text = cursor.slice_from(start);
                let kind = match text {
                    "true" => SyntaxKind::TRUE_KW,
                    "false" => SyntaxKind::FALSE_KW,
                    "null" => SyntaxKind::NULL_KW,
                    _ => SyntaxKind::IDENT,
                };
                tokens.push((kind, text));
            }
            _ => {
                cursor.bump();
                tokens.push((SyntaxKind::TEXT_CHUNK, cursor.slice_from(start)));
            }
        }
    }

    tokens
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_peeking_and_bumping() {
        let mut cursor = Cursor::new("abc\ndef");
        assert_eq!(cursor.peek(), Some('a'));
        assert_eq!(cursor.peek_at(1), Some('b'));
        assert_eq!(cursor.bump(), Some('a'));
        assert_eq!(cursor.pos(), 1);
        assert!(!cursor.is_eof());

        assert!(cursor.eat_char('b'));
        assert_eq!(cursor.bump(), Some('c'));
        assert_eq!(cursor.bump(), Some('\n'));
        assert_eq!(cursor.line_col(cursor.pos()), (2, 1));

        assert_eq!(cursor.eat_while(|c| c.is_alphabetic()), "def");
        assert!(cursor.is_eof());
    }

    #[test]
    fn test_cursor_eating_helpers() {
        let mut cursor = Cursor::new("  hello <world>");
        assert_eq!(cursor.eat_whitespace(), "  ");
        assert_eq!(cursor.peek(), Some('h'));

        assert_eq!(cursor.eat_if(|c| c == 'h'), Some('h'));
        assert!(cursor.eat_str("ello"));
        assert_eq!(cursor.eat_whitespace(), " ");

        let tag = cursor.eat_until_char('>');
        assert_eq!(tag, "<world");
        assert!(cursor.eat_char('>'));
        assert!(cursor.is_eof());
    }

    #[test]
    fn test_cursor_positions_and_spans() {
        let src = "line1\nline2";
        let mut cursor = Cursor::new(src);
        let start = cursor.pos();
        assert_eq!(cursor.line_col(0), (1, 1));

        cursor.eat_until_char('\n');
        let line1_span = cursor.span_from(start);
        assert_eq!(line1_span.start.line, 1);
        assert_eq!(line1_span.start.column, 1);
        assert_eq!(line1_span.end.line, 1);
        assert_eq!(line1_span.end.column, 6);

        cursor.bump(); // consume '\n'
        let p2 = cursor.current_position();
        assert_eq!(p2.line, 2);
        assert_eq!(p2.column, 1);
    }

    #[test]
    fn test_tokenize_lossless_roundtrip() {
        let samples = [
            "#[ Heading 1 ]\n\nSome paragraph with @tag(id: 123){ key: \"value\" }[ Content ]\n",
            "// A comment\n@config(format:json)+++\n{\n  \"format\": { \"meta\": \"yaml\" }\n}\n+++\n",
            "- Item 1\n- Item 2\n  - Subitem 2.1\n\n@task[ Do something ]\n",
        ];

        for sample in samples {
            let tokens = tokenize(sample);
            let roundtripped: String = tokens.into_iter().map(|(_, text)| text).collect();
            assert_eq!(roundtripped, sample);
        }
    }
}
