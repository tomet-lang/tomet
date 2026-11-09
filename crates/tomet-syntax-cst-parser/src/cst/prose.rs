//! Running text: paragraphs, and what nests in them.
//!
//! Escapes, emphasis, strong, strikeout, code spans, autolinks and inline forms, and where a
//! paragraph or a span ends. What opens a span and where an address ends is decided by the
//! functions in [`crate::inline`], so the tree and the AST cannot drift.

use super::CstParser;
use crate::inline::{
    DELIMITERS, autolink_end, autolink_scheme_at, delimited_probe, escaped_run_len, is_boundary,
};
use tomet_cst::SyntaxKind as K;
use tomet_lexer::Cursor;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InlineStop {
    /// Up to and including the newline.
    Line,
    /// Up to, not including, the newline: the text of a line without the line break.
    LineText,
    /// Up to and including the newline that ends the paragraph -- or,
    /// when [`CstParser::in_bracket`], the enclosing group's own closing
    /// `]`, depth-counted so a literal `[`/`]` typed as plain prose
    /// (`array[0]`) doesn't end it early.
    Paragraph,
    /// Up to, not including, the closing delimiter of an `EMPHASIS`,
    /// `STRONG` or `STRIKEOUT`; or the end of the paragraph, or the `]` that
    /// closes the group the span stands in, if that comes first.
    Delim(&'static str),
    /// Up to, not including, the byte at this offset: where the groups a sugar
    /// body ends in begin.
    Offset(usize),
}

impl<'a, 't> CstParser<'a, 't> {
    pub(super) fn parse_paragraph(&mut self) {
        self.builder.start_node(K::PARAGRAPH.into());
        self.parse_inline(InlineStop::Paragraph);
        self.builder.finish_node();
    }

    /// At a `NEWLINE` inside a paragraph: does the paragraph end here?
    fn paragraph_ends_after_newline(&self) -> bool {
        let i = self.skip_ws_from(self.pos + 1);
        matches!(self.kind_at(i), None | Some(K::NEWLINE)) || self.block_starts_at(i)
    }

    pub(super) fn parse_inline(&mut self, stop: InlineStop) {
        let mut depth = 0usize;
        while let Some(k) = self.current_kind() {
            // The text of a `|` run stops where the run does.
            if self.at_run_end() {
                break;
            }
            match stop {
                InlineStop::Offset(at) => {
                    if self.start_of(self.pos) >= at {
                        break;
                    }
                }
                InlineStop::Line if k == K::NEWLINE => {
                    self.bump();
                    break;
                }
                InlineStop::LineText if k == K::NEWLINE => {
                    break;
                }
                InlineStop::Paragraph => {
                    if self.in_bracket {
                        match k {
                            K::R_BRACKET if depth == 0 => break,
                            K::R_BRACKET => depth -= 1,
                            K::L_BRACKET => depth += 1,
                            _ => {}
                        }
                    }
                    if k == K::NEWLINE && self.paragraph_ends_after_newline() {
                        self.bump();
                        break;
                    }
                }
                InlineStop::Delim(d) => {
                    // A blank line ends the span unclosed; so does the `]` of the
                    // group it stands in.
                    if k == K::NEWLINE && self.blank_line_follows(self.pos) {
                        break;
                    }
                    if self.in_bracket {
                        match k {
                            K::R_BRACKET if depth == 0 => break,
                            K::R_BRACKET => depth -= 1,
                            K::L_BRACKET => depth += 1,
                            _ => {}
                        }
                    }
                    if self.closer_here(d) {
                        break;
                    }
                }
                _ => {}
            }
            if !(self.try_parse_escape()
                || self.try_parse_code_span()
                || self.try_parse_autolink(stop)
                || self.try_parse_inline_form()
                || self.try_parse_delimited())
            {
                if self.current_kind() == Some(K::QUOTE) {
                    self.bump_as(K::TEXT_CHUNK);
                } else {
                    self.bump();
                }
            }
        }
    }

    fn try_parse_escape(&mut self) -> bool {
        let start = self.start_of(self.pos);
        let Some(len) = escaped_run_len(self.src.as_bytes(), start) else {
            return false;
        };
        let end = start + len;
        self.builder.start_node(K::ESCAPE.into());
        while self.start_of(self.pos) < end && !self.is_eof() {
            if let Some(&(kind, text)) = self.tokens.get(self.pos) {
                self.builder.token(kind.into(), text);
                self.pos += 1;
            }
        }
        self.builder.finish_node();
        true
    }

    /// After the `NEWLINE` at `i`: is the next line empty (or the text over)?
    fn blank_line_follows(&self, i: usize) -> bool {
        let j = self.skip_ws_from(i + 1);
        matches!(self.kind_at(j), None | Some(K::NEWLINE))
    }

    /// Does the closing `delim` of the span being read start at the current token?
    fn closer_here(&self, delim: &str) -> bool {
        // A comment is read whole: a delimiter inside it closes nothing.
        if self.current_kind() == Some(K::COMMENT) {
            return false;
        }
        self.closes_at(self.start_of(self.pos), delim)
    }

    /// A closing delimiter stands at `at`: `delim` there, and not right after
    /// a space.
    fn closes_at(&self, at: usize, delim: &str) -> bool {
        self.src[at..].starts_with(delim) && !is_boundary(self.src[..at].chars().next_back())
    }

    /// `*em*`, `_em_`, `**strong**`, `__strong__` or `~~struck~~` at the cursor.
    fn try_parse_delimited(&mut self) -> bool {
        let start = self.start_of(self.pos);
        if !matches!(self.src.as_bytes().get(start), Some(b'*' | b'_' | b'~')) {
            return false;
        }
        let mut cur = Cursor::new(self.src);
        cur.set_pos(start);
        for (delim, name) in DELIMITERS {
            if !delimited_probe(&cur, delim, &self.delimiters) {
                continue;
            }
            // Past the limit the delimiter is not read; the tree marks it.
            if self.too_deep() {
                self.builder.start_node(K::TOO_DEEP.into());
                self.bump_delimiter(delim);
                self.builder.finish_node();
                return true;
            }
            let kind = match name {
                "strong" => K::STRONG,
                "strikeout" => K::STRIKEOUT,
                _ => K::EMPHASIS,
            };
            self.builder.start_node(kind.into());
            self.bump_delimiter(delim);
            self.depth += 1;
            self.parse_inline(InlineStop::Delim(delim));
            self.depth -= 1;
            if self.closes_at(self.start_of(self.pos), delim) {
                self.bump_delimiter(delim);
            }
            self.builder.finish_node();
            return true;
        }
        false
    }

    /// Emits the delimiter at the cursor one character per token, each as the
    /// token it is.
    fn bump_delimiter(&mut self, delim: &str) {
        for c in delim.chars() {
            self.bump_as(match c {
                '*' => K::STAR,
                '_' => K::UNDERSCORE,
                '~' => K::TILDE,
                _ => K::TEXT_CHUNK,
            });
        }
    }

    /// `` `code` `` at the cursor: a single backtick, then the next backtick on
    /// the same line with at least one character between them. What stands
    /// between is code, read as nothing else -- the tokenizer's idea of a
    /// comment or a string included.
    fn try_parse_code_span(&mut self) -> bool {
        if self.current_kind() != Some(K::BACKTICK) {
            return false;
        }
        let bytes = self.src.as_bytes();
        let start = self.start_of(self.pos);
        let inner_start = start + 1;
        let mut i = inner_start;
        while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r' | b'`') {
            i += 1;
        }
        if i == inner_start || bytes.get(i) != Some(&b'`') {
            return false;
        }
        let inner_end = i;
        self.builder.start_node(K::CODE_SPAN.into());
        self.bump_as(K::BACKTICK);
        while self.start_of(self.pos) < inner_end {
            self.bump_as(K::TEXT_CHUNK);
        }
        self.bump_as(K::BACKTICK);
        self.builder.finish_node();
        true
    }

    /// A bare `https://`, `http://` or `mailto:` address at the cursor.
    fn try_parse_autolink(&mut self, stop: InlineStop) -> bool {
        let start = self.start_of(self.pos);
        let mut cur = Cursor::new(self.src);
        cur.set_pos(start);
        let Some(scheme_len) = autolink_scheme_at(&cur) else {
            return false;
        };
        let mut scanned = start;
        for (off, ch) in self.src[start..].char_indices() {
            let at = start + off;
            if matches!(ch, ' ' | '\t' | '\n' | '\r') || ch < ' ' {
                break;
            }
            if let InlineStop::Delim(d) = stop
                && self.closes_at(at, d)
            {
                break;
            }
            scanned = at + ch.len_utf8();
        }
        let Some(end) = autolink_end(self.src, start, scheme_len, scanned) else {
            return false;
        };
        self.builder.start_node(K::AUTOLINK.into());
        while self.start_of(self.pos) < end && !self.is_eof() {
            match self.current_kind() {
                // `//` inside an address is part of it, not a comment.
                Some(K::COMMENT) => self.bump_as(K::TEXT_CHUNK),
                _ => self.bump(),
            }
        }
        self.builder.finish_node();
        true
    }
}
