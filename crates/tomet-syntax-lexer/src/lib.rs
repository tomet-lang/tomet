//! =[ tomet-lexer ]
//!
//! A minimal byte-position cursor ([`Cursor`]) over `&str` source text.
//!
//! =[ Overview & Design ]
//!
//! - (Context-Sensitive Scanning)
//!   | Tomet's grammar is context-sensitive -- `<`, `@`, `(`, `[`, `{` are only
//!   | structural right after specific triggers (an element sigil/name), and plain
//!   | prose text otherwise. Scanning directly with a lightweight cursor is more efficient
//!   | and flexible than pre-tokenizing into a context-free token stream.
//! - (Zero-Copy & Copyable)
//!   | [`Cursor`] holds only `&'a str` and byte offset `pos: usize`, making it `Copy`
//!   | and strictly zero-copy.
//! - (Core Capabilities)
//!   | Scanning & Consumption: `bump()`, `eat_char()`, `eat_str()`, `eat_while()`, `eat_whitespace()`.
//!   | Lookahead & Peeking: `peek()`, `peek_at()`, `starts_with()`.
//!   | Span Mapping: `span_from()`, `column()`. Spans are byte offsets; lines and columns are not
//!   | computed while scanning -- [`LineIndex`] resolves them from offsets on demand.

use tomet_ast::Span;

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

    /// The 1-based column, in `char`s, of the cursor within its line. Counts back
    /// to the previous line start, so it costs the length of the line and never
    /// the length of the file.
    pub fn column(&self) -> usize {
        let line_start = self.src[..self.pos].rfind('\n').map_or(0, |i| i + 1);
        self.src[line_start..self.pos].chars().count() + 1
    }

    /// Returns a [`Span`] from `start_pos` byte offset to the current cursor position.
    pub fn span_from(&self, start_pos: usize) -> Span {
        Span::new(start_pos, self.pos)
    }
}

/// A 1-based line and a 1-based column, the column counted in `char`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct LineCol {
    pub line: usize,
    pub column: usize,
}

/// Resolves byte offsets of one source text to lines and columns, and back.
///
/// Spans and errors carry byte offsets only; a line and column is a view of
/// the text, built here when something has to show one (a diagnostic, an
/// editor position). Building the index is one pass over the text, a lookup is
/// a binary search plus a count of the characters in that one line, and
/// nothing about it is paid while parsing.
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    src: &'a str,
    /// Byte offset at which each line starts; `starts[0]` is always `0`.
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { src, starts }
    }

    pub fn src(&self) -> &'a str {
        self.src
    }

    /// Number of lines. A text ending in `\n` has an empty last line.
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// Byte offset at which the 1-based `line` starts, or `None` if there is no
    /// such line.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.starts.get(line.checked_sub(1)?).copied()
    }

    /// `offset` brought into the text: clamped to its length, and moved back to
    /// the start of the character it falls inside.
    pub fn clamp(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.src.len());
        while !self.src.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// The line and column of byte `offset`. An offset past the end resolves
    /// to the end; one inside a multi-byte character resolves to that
    /// character's start.
    pub fn line_col(&self, offset: usize) -> LineCol {
        let offset = self.clamp(offset);
        let line = self.starts.partition_point(|&start| start <= offset) - 1;
        let column = self.src[self.starts[line]..offset].chars().count() + 1;
        LineCol {
            line: line + 1,
            column,
        }
    }

    /// The byte offset of a 1-based line and column, or `None` if the line
    /// does not exist or the column is past the end of that line (the column
    /// just after the last character is allowed). The inverse of
    /// [`line_col`](Self::line_col).
    pub fn offset(&self, position: LineCol) -> Option<usize> {
        let line = position.line.checked_sub(1)?;
        let column = position.column.checked_sub(1)?;
        let start = *self.starts.get(line)?;
        let end = self.starts.get(line + 1).copied().unwrap_or(self.src.len());
        let text = &self.src[start..end];
        let text = text.strip_suffix('\n').unwrap_or(text);
        let mut chars = text.char_indices();
        match chars.nth(column) {
            Some((i, _)) => Some(start + i),
            None if text.chars().count() == column => Some(start + text.len()),
            None => None,
        }
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
            // `//` starts a comment only after whitespace (or at the start of
            // the source), as in the inline parser: `https://x` is not one.
            '/' if cursor.starts_with("//")
                && src[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|p| matches!(p, ' ' | '\t' | '\n' | '\r')) =>
            {
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
            // An apostrophe inside a word (`don't`) is text, not a quote.
            '\'' if src[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric) =>
            {
                cursor.bump();
                tokens.push((SyntaxKind::TEXT_CHUNK, cursor.slice_from(start)));
            }
            // A string ends at its closing quote or, unterminated, at the end
            // of the line: a stray quote must not swallow the rest of the file.
            '"' | '\'' => {
                let quote = c;
                cursor.bump();
                let mut escaped = false;
                while let Some(ch) = cursor.peek() {
                    if ch == '\n' {
                        break;
                    }
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
    #[test]
    fn double_slash_is_a_comment_only_after_a_boundary() {
        let kinds =
            |src| -> Vec<SyntaxKind> { tokenize(src).into_iter().map(|(k, _)| k).collect() };
        assert!(kinds("a // c").contains(&SyntaxKind::COMMENT));
        assert!(kinds("// c").contains(&SyntaxKind::COMMENT));
        assert!(!kinds("https://x").contains(&SyntaxKind::COMMENT));
    }

    #[test]
    fn strings_stop_at_the_line_and_apostrophes_in_words_are_text() {
        let toks = tokenize("\"open\nnext");
        assert_eq!(toks[0], (SyntaxKind::STRING_LITERAL, "\"open"));
        assert_eq!(toks[1].0, SyntaxKind::NEWLINE);
        assert!(
            tokenize("don't")
                .iter()
                .all(|(k, _)| *k != SyntaxKind::STRING_LITERAL)
        );
    }

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
        assert_eq!(
            LineIndex::new("abc\ndef").line_col(cursor.pos()),
            LineCol { line: 2, column: 1 }
        );

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
    fn test_cursor_spans_are_byte_offsets() {
        let mut cursor = Cursor::new("line1\nline2");
        let start = cursor.pos();
        cursor.eat_until_char('\n');
        assert_eq!(cursor.span_from(start), Span::new(0, 5));
        assert!(cursor.span_from(start).exact_eq(&Span::new(0, 5)));
        cursor.bump();
        assert_eq!(cursor.pos(), 6);
    }

    fn lc(line: usize, column: usize) -> LineCol {
        LineCol { line, column }
    }

    #[test]
    fn test_line_index_resolves_lines_and_columns() {
        let src = "line1\nline2";
        let index = LineIndex::new(src);
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_col(0), lc(1, 1));
        assert_eq!(index.line_col(5), lc(1, 6));
        assert_eq!(index.line_col(6), lc(2, 1));
        assert_eq!(index.line_col(src.len()), lc(2, 6));
        // Past the end clamps to the end.
        assert_eq!(index.line_col(999), lc(2, 6));
    }

    #[test]
    fn test_line_index_counts_chars_not_bytes() {
        // Each of these is 3 bytes.
        let src = "あいう\nえお";
        let index = LineIndex::new(src);
        assert_eq!(index.line_col(3), lc(1, 2));
        assert_eq!(index.line_col(9), lc(1, 4));
        assert_eq!(index.line_col(10), lc(2, 1));
        // Inside a character: resolves to that character's start.
        assert_eq!(index.line_col(4), lc(1, 2));
    }

    #[test]
    fn test_line_index_offset_is_the_inverse() {
        let src = "あいう\nabc\n\nz";
        let index = LineIndex::new(src);
        for offset in (0..=src.len()).filter(|&o| src.is_char_boundary(o)) {
            assert_eq!(
                index.offset(index.line_col(offset)),
                Some(offset),
                "offset {offset}"
            );
        }
        assert_eq!(index.offset(lc(0, 1)), None);
        assert_eq!(index.offset(lc(1, 5)), None);
        assert_eq!(index.offset(lc(9, 1)), None);
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
