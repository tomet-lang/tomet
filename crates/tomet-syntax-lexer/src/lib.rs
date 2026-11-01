//! =[ tomet-lexer ]
//!
//! Byte-position cursor ([`Cursor`]) and lossless tokenizer ([`tokenize`]) for Tomet.
//!
//! =[ Overview & Design ]
//!
//! - (Context-Free Tokenization)
//!   | The tokenizer is strictly context-free: a token decision never depends on
//!   | anything past the token itself and at most one character of lookbehind. The
//!   | tokenizer never scans ahead across lines, never searches for closing markers
//!   | like `*/`, and never tracks grammatical nesting state.
//! - (Parser Assembly)
//!   | The tokenizer emits fine-grained token pieces. Where the grammar requires
//!   | larger semantic tokens -- strings in value contexts (`STRING_LITERAL`),
//!   | compound identifiers (`IDENT`), and comments (`COMMENT` assembled from line
//!   | openers `//` or block openers `/*` to `*/` or newline) -- the parser assembles
//!   | them. In running prose, quotes and individual comment pieces outside comment
//!   | contexts remain plain text. Inside strings and code spans, comment pieces are
//!   | accepted as plain text.
//! - (Zero-Copy & Copyable)
//!   | [`Cursor`] holds only `&'a str` and byte offset `pos: usize`, making it `Copy`
//!   | and strictly zero-copy.
//! - (Lossless Tokenization)
//!   | [`tokenize`] splits source text into `(SyntaxKind, &str)` tokens preserving 100%
//!   | of input characters and whitespace for CST construction. Concatenating all tokens
//!   | exactly reproduces the original source.
//! - (Lookbehind Invariants)
//!   | Word-internal apostrophes (`don't`) are emitted as text rather than quote tokens.
//!   | `//` starts a line comment token (`COMMENT_LINE_OPEN`) only after whitespace or
//!   | at the start of source.
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
        let start_of_line = self.src[..self.pos]
            .bytes()
            .rposition(|b| b == b'\n')
            .map_or(0, |i| i + 1);
        self.src[start_of_line..self.pos].chars().count() + 1
    }

    /// Returns a [`Span`] from `start_pos` byte offset to the current cursor position.
    pub fn span_from(&self, start_pos: usize) -> Span {
        Span::new(start_pos, self.pos)
    }
}

use tomet_cst::SyntaxKind;

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
            // `//` starts a line comment only after whitespace (or at the start of
            // the source): `https://x` is not one. Emits the opener token; the parser
            // assembles the comment.
            '/' if cursor.starts_with("//")
                && src[..start]
                    .chars()
                    .next_back()
                    .is_none_or(|p| matches!(p, ' ' | '\t' | '\n' | '\r')) =>
            {
                cursor.bump();
                cursor.bump();
                tokens.push((SyntaxKind::COMMENT_LINE_OPEN, cursor.slice_from(start)));
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
            '_' => {
                cursor.bump();
                tokens.push((SyntaxKind::UNDERSCORE, cursor.slice_from(start)));
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
            '"' | '\'' => {
                cursor.bump();
                tokens.push((SyntaxKind::QUOTE, cursor.slice_from(start)));
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
    c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric()
}

#[cfg(test)]
mod tests {

    #[test]
    fn double_slash_is_a_comment_only_after_a_boundary() {
        let kinds =
            |src| -> Vec<SyntaxKind> { tokenize(src).into_iter().map(|(k, _)| k).collect() };
        assert!(kinds("a // c").contains(&SyntaxKind::COMMENT_LINE_OPEN));
        assert!(kinds("// c").contains(&SyntaxKind::COMMENT_LINE_OPEN));
        assert!(!kinds("https://x").contains(&SyntaxKind::COMMENT_LINE_OPEN));
    }

    #[test]
    fn comment_pieces_are_emitted_by_tokenizer() {
        let toks = tokenize("a /* b */ c // d\n");
        assert!(toks.contains(&(SyntaxKind::SLASH, "/")));
        assert!(toks.contains(&(SyntaxKind::STAR, "*")));
        assert!(toks.contains(&(SyntaxKind::COMMENT_LINE_OPEN, "//")));
        assert_eq!(
            toks.iter().map(|(_, t)| *t).collect::<String>(),
            "a /* b */ c // d\n"
        );
    }

    #[test]
    fn a_slash_that_opens_nothing_is_a_slash() {
        let kinds =
            |src| -> Vec<SyntaxKind> { tokenize(src).into_iter().map(|(k, _)| k).collect() };
        assert!(kinds("a / b").contains(&SyntaxKind::SLASH));
        assert!(kinds("a */ b").contains(&SyntaxKind::STAR));
        assert!(kinds("a */ b").contains(&SyntaxKind::SLASH));
    }

    #[test]
    fn quotes_are_single_tokens_and_apostrophes_in_words_are_text() {
        let toks = tokenize("\"open\nnext");
        assert_eq!(toks[0], (SyntaxKind::QUOTE, "\""));
        assert_eq!(toks[1], (SyntaxKind::IDENT, "open"));
        assert_eq!(toks[2].0, SyntaxKind::NEWLINE);
        assert!(
            tokenize("don't")
                .iter()
                .all(|(k, _)| *k != SyntaxKind::QUOTE)
        );
        let delim_toks = |src| -> Vec<SyntaxKind> {
            tokenize(src).into_iter().map(|(k, _)| k).collect()
        };
        assert_eq!(
            delim_toks("__strong__"),
            vec![
                SyntaxKind::UNDERSCORE,
                SyntaxKind::UNDERSCORE,
                SyntaxKind::IDENT,
                SyntaxKind::UNDERSCORE,
                SyntaxKind::UNDERSCORE,
            ]
        );
        assert_eq!(
            delim_toks("a-b"),
            vec![SyntaxKind::IDENT, SyntaxKind::MINUS, SyntaxKind::IDENT]
        );
        assert_eq!(
            delim_toks("a.b"),
            vec![SyntaxKind::IDENT, SyntaxKind::DOT, SyntaxKind::IDENT]
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
