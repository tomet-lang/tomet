//! Reading and emitting tokens.
//!
//! The kind of a token as the grammar sees it (a later line's `|` marker is white space, a comment
//! is one thing), the bumps that put tokens into the green tree, and the position helpers (line
//! starts, columns, white space runs) every other file leans on. Nothing here knows a construct.

use super::CstParser;
use tomet_cst::SyntaxKind;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    pub(super) fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn is_block_comment_open_at(&self, i: usize) -> bool {
        if self.raw_kind_at(i) == Some(K::SLASH) && self.raw_kind_at(i + 1) == Some(K::STAR) {
            let start_slash = self.start_of(i);
            let start_star = self.start_of(i + 1);
            start_star == start_slash + 1
        } else {
            false
        }
    }

    /// The kind of token `i` as the grammar sees it: a run's marker on a later
    /// line is white space.
    pub(super) fn kind_at(&self, i: usize) -> Option<SyntaxKind> {
        match self.raw_kind_at(i)? {
            K::PIPE if !self.markers.is_empty() && self.markers.contains(&self.start_of(i)) => {
                Some(K::WHITESPACE)
            }
            K::COMMENT_LINE_OPEN => {
                if self.in_raw_block {
                    Some(K::TEXT_CHUNK)
                } else {
                    Some(K::COMMENT)
                }
            }
            K::SLASH if self.is_block_comment_open_at(i) => {
                if self.in_raw_block {
                    Some(K::SLASH)
                } else {
                    Some(K::COMMENT)
                }
            }
            k => Some(k),
        }
    }

    /// The kind of token `i` as the tokenizer read it, markers included.
    pub(super) fn raw_kind_at(&self, i: usize) -> Option<SyntaxKind> {
        self.tokens.get(i).map(|(k, _)| *k)
    }

    pub(super) fn text_at(&self, i: usize) -> &'a str {
        self.tokens.get(i).map_or("", |(_, t)| *t)
    }

    pub(super) fn current_kind(&self) -> Option<SyntaxKind> {
        self.kind_at(self.pos)
    }

    /// How many tokens form the comment starting at `from`.
    ///
    /// For line comments (`COMMENT_LINE_OPEN`), runs to the next newline.
    /// For block comments (`SLASH` + `STAR`), runs to the matching closer (`STAR` + `SLASH`),
    /// or if unclosed, to the end of the line (newline).
    pub(super) fn comment_len_at(&self, from: usize) -> usize {
        if self.raw_kind_at(from) == Some(K::COMMENT_LINE_OPEN) {
            let mut j = from + 1;
            while let Some(k) = self.raw_kind_at(j) {
                if k == K::NEWLINE {
                    break;
                }
                j += 1;
            }
            return j - from;
        }
        if self.is_block_comment_open_at(from) {
            let next_close = self.block_closers.partition_point(|&c| c <= from + 2);
            if let Some(&close_tok) = self.block_closers.get(next_close) {
                return (close_tok + 1) - from;
            } else {
                // Block comment never closed: runs to the end of the line.
                let mut j = from + 1;
                while let Some(k) = self.raw_kind_at(j) {
                    if k == K::NEWLINE {
                        break;
                    }
                    j += 1;
                }
                return j - from;
            }
        }
        0
    }

    /// Assembles a single `COMMENT` token in the green tree spanning all constituent tokens
    /// of the comment, advancing parser position past them. Unclosed block comments are
    /// wrapped in an `ERROR` node.
    fn bump_comment(&mut self) {
        let count = self.comment_len_at(self.pos);
        if count == 0 {
            self.bump();
            return;
        }
        let is_unclosed_block = self.is_block_comment_open_at(self.pos)
            && self.block_closers.partition_point(|&c| c <= self.pos + 2)
                >= self.block_closers.len();

        let start = self.start_of(self.pos);
        let end = if self.pos + count >= self.tokens.len() {
            self.src.len()
        } else {
            self.start_of(self.pos + count)
        };
        if is_unclosed_block {
            self.builder.start_node(K::ERROR.into());
            self.builder.token(K::COMMENT.into(), &self.src[start..end]);
            self.builder.finish_node();
        } else {
            self.builder.token(K::COMMENT.into(), &self.src[start..end]);
        }
        self.pos += count;
    }

    pub(super) fn bump(&mut self) {
        if let Some(&(kind, text)) = self.tokens.get(self.pos) {
            match kind {
                K::COMMENT_LINE_OPEN if !self.in_raw_block => {
                    self.bump_comment();
                }
                K::SLASH if self.is_block_comment_open_at(self.pos) && !self.in_raw_block => {
                    self.bump_comment();
                }
                K::COMMENT_LINE_OPEN => {
                    self.builder.token(K::TEXT_CHUNK.into(), text);
                    self.pos += 1;
                }
                K::PIPE if self.markers.contains(&self.start_of(self.pos)) => {
                    self.builder.token(K::MARGIN.into(), text);
                    self.pos += 1;
                }
                _ => {
                    self.builder.token(kind.into(), text);
                    self.pos += 1;
                }
            }
        }
    }

    /// Emits the current token as `kind`, whatever the tokenizer called it.
    pub(super) fn bump_as(&mut self, kind: SyntaxKind) {
        if let Some(&(_, text)) = self.tokens.get(self.pos) {
            self.builder.token(kind.into(), text);
            self.pos += 1;
        }
    }

    /// The byte offset in `src` at which token `i` starts (`src.len()` past
    /// the last one). Tokens are slices of `src`, so this is pointer
    /// arithmetic.
    pub(super) fn start_of(&self, i: usize) -> usize {
        match self.tokens.get(i) {
            Some((_, text)) => text.as_ptr() as usize - self.src.as_ptr() as usize,
            None => self.tokens.last().map_or(self.src.len(), |(_, text)| {
                text.as_ptr() as usize - self.src.as_ptr() as usize + text.len()
            }),
        }
    }

    pub(super) fn token_at_offset(&self, offset: usize) -> usize {
        self.tokens
            .partition_point(|(_, t)| (t.as_ptr() as usize - self.src.as_ptr() as usize) < offset)
    }

    pub(super) fn bump_string_literal(&mut self) {
        let start = self.start_of(self.pos);
        let bytes = self.src.as_bytes();
        if start >= bytes.len() {
            self.bump();
            return;
        }
        let quote = bytes[start];
        let mut cur = start + 1;
        let mut escaped = false;
        let mut line_end = None;
        let mut found_closing_quote = false;

        while cur < bytes.len() {
            let b = bytes[cur];
            if b == b'\n' && line_end.is_none() {
                line_end = Some(cur);
            }
            cur += 1;
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == quote {
                found_closing_quote = true;
                break;
            }
        }

        let end = if found_closing_quote {
            cur
        } else {
            line_end.unwrap_or(cur)
        };

        let tok_end = self.token_at_offset(end);
        let count = tok_end.saturating_sub(self.pos).max(1);
        self.builder
            .token(K::STRING_LITERAL.into(), &self.src[start..end]);
        self.pos += count;
    }

    pub(super) fn bump_while(&mut self, kind: SyntaxKind) {
        while self.current_kind() == Some(kind) {
            self.bump();
        }
    }

    pub(super) fn skip_ws_from(&self, mut i: usize) -> usize {
        while self.kind_at(i) == Some(K::WHITESPACE) {
            i += 1;
        }
        i
    }

    pub(super) fn run_len(&self, i: usize, kind: SyntaxKind) -> usize {
        let mut n = 0;
        while self.kind_at(i + n) == Some(kind) {
            n += 1;
        }
        n
    }

    /// Index of the first token from `i` that is not white space. Markers
    /// are not skipped: this reads the tokens as the tokenizer made them.
    pub(super) fn skip_raw_ws(&self, mut i: usize) -> usize {
        while self.raw_kind_at(i) == Some(K::WHITESPACE) {
            i += 1;
        }
        i
    }

    /// Bumps the white space and comments at the cursor.
    pub(super) fn bump_blanks(&mut self) {
        while matches!(self.current_kind(), Some(K::WHITESPACE | K::COMMENT)) {
            self.bump();
        }
    }

    /// Reads the rest of the line, its `NEWLINE` included.
    pub(super) fn bump_line(&mut self) {
        while let Some(k) = self.current_kind() {
            self.bump();
            if k == K::NEWLINE {
                break;
            }
        }
    }

    /// Index of the first token from `i` that is not white space or a comment.
    pub(super) fn past_blanks(&self, mut i: usize) -> usize {
        while let Some(k) = self.kind_at(i) {
            if k == K::WHITESPACE {
                i += 1;
            } else if k == K::COMMENT {
                i += self.comment_len_at(i);
            } else {
                break;
            }
        }
        i
    }

    /// Is the cursor the first thing on its line? Inside a `|` run the
    /// opener's `|` is part of the line's left edge, as the markers of the
    /// lines after it are: a form written after it stands as a block.
    pub(super) fn at_line_start(&self) -> bool {
        self.line_start_at(self.pos)
    }

    /// [`CstParser::at_line_start`] for token `i`.
    pub(super) fn line_start_at(&self, i: usize) -> bool {
        let in_run = !self.pipe_cols.is_empty();
        let mut i = i;
        while i > 0
            && (self.kind_at(i - 1) == Some(K::WHITESPACE)
                || (in_run && self.kind_at(i - 1) == Some(K::PIPE)))
        {
            i -= 1;
        }
        i == 0 || self.kind_at(i - 1) == Some(K::NEWLINE)
    }

    /// The 1-based column, in characters, at which byte `at` stands in its
    /// line -- what `Cursor::column` says.
    pub(super) fn column_at(&self, at: usize) -> usize {
        let line_start = self.src[..at].rfind('\n').map_or(0, |i| i + 1);
        self.src[line_start..at].chars().count() + 1
    }

    pub(super) fn is_opener(kind: Option<SyntaxKind>) -> bool {
        matches!(kind, Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE))
    }
}
