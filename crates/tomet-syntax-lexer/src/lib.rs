//! =[ tomet-lexer ]
//!
//! Byte-position cursor ([`Cursor`]) and lossless tokenizer ([`tokenize`]) for Tomet.
//!
//! =[ Overview & Design ]
//!
//! - (Context-Sensitive Scanning)
//!   | Tomet's grammar is context-sensitive -- `<`, `@`, `(`, `[`, `{` are only
//!   | structural right after specific triggers (an element sigil/name), and plain
//!   | prose text otherwise. [`Cursor`] provides scanning and consumption helpers.
//! - (Zero-Copy & Copyable)
//!   | [`Cursor`] holds only `&'a str` and byte offset `pos: usize`, making it `Copy`
//!   | and strictly zero-copy.
//! - (Lossless Tokenization)
//!   | [`tokenize`] splits source text into `(SyntaxKind, &str)` tokens preserving 100%
//!   | of input characters and whitespace for CST construction.
//! - (Core Capabilities)
//!   | Scanning & Consumption: `bump()`, `eat_char()`, `eat_str()`, `eat_while()`, `eat_whitespace()`.
//!   | Lookahead & Peeking: `peek()`, `peek_at()`, `starts_with()`.
//!   | Span Mapping: `span_from()`, `column()`. Spans are byte offsets; lines and columns are not
//!   | computed while scanning -- resolve them on demand using `tomet-position`.

use tomet_span::Span;

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
            // `/* ... */`, anywhere the inline parser skips one, up to the
            // first `*/` after the opening (they do not nest). The parser
            // refuses one that is never closed; a lossless tokenizer cannot,
            // so it runs to the end, which is what an editor shows while the
            // closing `*/` is not typed yet.
            '/' if cursor.starts_with("/*") => {
                let len = src[start + 2..]
                    .find("*/")
                    .map_or(src.len() - start, |i| i + 4);
                cursor.set_pos(start + len);
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
    fn a_block_comment_runs_to_the_first_closer_and_does_not_nest() {
        let comments = |src| -> Vec<String> {
            tokenize(src)
                .into_iter()
                .filter(|(k, _)| *k == SyntaxKind::COMMENT)
                .map(|(_, t)| t.to_string())
                .collect()
        };
        assert_eq!(comments("a /* b */ c"), ["/* b */"]);
        assert_eq!(comments("a/* b */c"), ["/* b */"]);
        assert_eq!(comments("/* a\nb */ c"), ["/* a\nb */"]);
        assert_eq!(comments("/* a /* b */ c */"), ["/* a /* b */"]);
        // `/*/` does not close itself.
        assert_eq!(comments("/*/ x */"), ["/*/ x */"]);
        // Never closed: to the end, and the text stays whole.
        assert_eq!(comments("a /* b\nc"), ["/* b\nc"]);
        assert_eq!(
            tokenize("a /* b\nc")
                .iter()
                .map(|(_, t)| *t)
                .collect::<String>(),
            "a /* b\nc"
        );
    }

    #[test]
    fn a_slash_that_opens_nothing_is_a_slash() {
        let kinds =
            |src| -> Vec<SyntaxKind> { tokenize(src).into_iter().map(|(k, _)| k).collect() };
        assert!(!kinds("a / b").contains(&SyntaxKind::COMMENT));
        assert!(!kinds("a */ b").contains(&SyntaxKind::COMMENT));
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
        assert_eq!(cursor.pos(), 4);
        assert_eq!(cursor.column(), 1);

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

    #[test]
    fn test_tokenize_lossless_roundtrip() {
        let samples = [
            "#[ Heading 1 ]\n\nSome paragraph with @tag(id: 123){ key: \"value\" }[ Content ]\n",
            "// A comment\n::config(format:json)+++\n{\n  \"format\": { \"meta\": \"yaml\" }\n}\n+++\n",
            "- Item 1\n- Item 2\n  - Subitem 2.1\n\n@task[ Do something ]\n",
        ];

        for sample in samples {
            let tokens = tokenize(sample);
            let roundtripped: String = tokens.into_iter().map(|(_, text)| text).collect();
            assert_eq!(roundtripped, sample);
        }
    }
}
