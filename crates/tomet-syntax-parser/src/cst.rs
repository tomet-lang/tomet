//! Resilient, lossless Concrete Syntax Tree (CST) parser for Tomet.
//!
//! Preserves 100% of source characters including trivia (whitespace, newlines, comments).
//!
//! The tree follows the current surface syntax, not the AST parser's exact
//! decisions: it never fails, and where the two could disagree it prefers
//! the reading that keeps more structure.
//!
//! ```text
//! ROOT
//!   SECTION                    `=[ T ]` and everything up to the next section
//!     SECTION_HEADING          of the same or a shallower level (they nest)
//!     PARAGRAPH | LIST | BLOCK_ELEMENT | CODE_BLOCK | THEMATIC_BREAK | SECTION ...
//! ```
//!
//! An element (`BLOCK_ELEMENT`, `INLINE_ELEMENT`) is a `SIGIL` followed by
//! its groups -- `ARGS` `( )`, `VALUE_DATA` `{ }`, `CONTENT` `[ ]`, at most
//! one of each -- then any `CONNECT` (`:name(...)`).
//! `${ ... }` is an `INTERP_EXPR`. Inside `ARGS`/`VALUE_DATA` an entry is a
//! `MAP_ENTRY` (`key: value`) or a `SEQ_ITEM`, and nested groups are
//! `MAP_LITERAL`, `SEQ_LITERAL` or (a call such as `list(...)`) `ARGS`.
//! `CONTENT` recurses into the same block grammar `ROOT`/`SECTION` use
//! (`SECTION`, `LIST`, `BLOCK_ELEMENT`, `CODE_BLOCK`, `THEMATIC_BREAK`,
//! `PARAGRAPH`), bounded by the group's own closing `]` instead of EOF or
//! a shallower section -- a block nested inside `[content]` (and its own
//! `#(id)`) is represented, not flattened into inline text.
//!
//! Running text carries `EMPHASIS`, `STRONG`, `STRIKEOUT`, `CODE_SPAN` and
//! `AUTOLINK` nodes where the AST parser reads an `em`, `strong`, `strikeout`,
//! `raw` or `link` element. What opens a span, and where an address ends, is
//! decided by the same functions in `inline.rs`, so the two cannot drift.
//! The tokenizer reads text without context (`_em_` is one identifier, a `//`
//! inside backticks starts a comment), so a token is split where such a span
//! starts or ends: the tree is the same text, cut at other places.
//!
//! A `|` run is `[content]` spelled without brackets, and so it is the same
//! node: a `CONTENT` that opens with a `PIPE` and has no closing `]`. It is an
//! element's content (`@quote| text`, or `|` on the line after the element), a
//! list item's (`-| text`), a heading's (`=| text`) or a `key: | prose` value.
//! It ends at the first line that does not repeat its marker in the same column
//! -- a blank line, or a `|` somewhere else, ends it too -- and the AST parser's
//! error for that last case is not one here. The `|` that begins each later
//! line is a `PIPE` token among the content's own, but the grammar reads it as
//! white space, so the blocks of a run are the blocks the bracketed form has.

use std::collections::HashSet;

use tomet_cst::{GreenNodeBuilder, SyntaxKind, SyntaxNode};
use tomet_lexer::{Cursor, tokenize};

use crate::inline::{DELIMITERS, autolink_end, autolink_scheme_at, delimited_probe, is_boundary};

use SyntaxKind as K;

/// Parse `src` into a lossless, resilient [`SyntaxNode`].
///
/// Guaranteed Invariant: `parse_cst(src).text().to_string() == src`.
pub fn parse_cst(src: &str) -> SyntaxNode {
    let raw_tokens = tokenize(src);
    let mut parser = CstParser::new(src, raw_tokens);
    parser.parse_root();
    SyntaxNode::new_root(parser.builder.finish())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineStop {
    /// Up to and including the newline.
    Line,
    /// Up to and including the newline that ends the paragraph -- or,
    /// when [`CstParser::in_bracket`], the enclosing group's own closing
    /// `]`, depth-counted so a literal `[`/`]` typed as plain prose
    /// (`array[0]`) doesn't end it early.
    Paragraph,
    /// Up to, not including, the closing delimiter of an `EMPHASIS`,
    /// `STRONG` or `STRIKEOUT`; or the end of the paragraph, or the `]` that
    /// closes the group the span stands in, if that comes first.
    Delim(&'static str),
}

struct CstParser<'a> {
    src: &'a str,
    /// The tokens of `src`, in order, each a slice of it. A token is split in
    /// two (see [`CstParser::split_at`]) where a span starts or ends inside it.
    tokens: Tokens<'a>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    /// Whether an element may claim a `:name(...)` group that follows it.
    /// A list item's own top-level element may not: the group is the item's.
    allow_connect: bool,
    /// Whether the block grammar currently being parsed (via
    /// [`CstParser::parse_body`]/[`CstParser::parse_inline`]) is bounded
    /// by a `[content]` group's closing `]` rather than EOF/a section.
    /// Toggled around [`CstParser::parse_content_group`]'s own call,
    /// the same way `allow_connect` is -- a nested `[...]` sets and
    /// restores it around its own body, so nesting depth is tracked by
    /// the call stack, not a counter here.
    in_bracket: bool,
    /// The columns of the `|` markers of the runs being read, outermost
    /// first: the `|` that opened each, which the run's later lines repeat.
    pipe_cols: Vec<usize>,
    /// Where each of those runs ends, as a byte offset into `src` (the
    /// `NEWLINE` after its last line). Only the innermost matters, and it is
    /// never past the one outside it.
    run_ends: Vec<usize>,
    /// The start of every `|` that is a run's marker on a later line rather
    /// than its opener. The parser does not see them: [`CstParser::kind_at`]
    /// answers `WHITESPACE` for them, which is what the AST parser does when
    /// it folds a marker away with the newline before it. The tree still
    /// gets them as `PIPE` tokens, because [`CstParser::bump`] emits what the
    /// tokenizer read.
    markers: HashSet<usize>,
}

/// The tokens of the text, kept as two stacks around the cursor.
///
/// A token is cut in two where a span starts or ends inside it, and the cut is
/// always at or just after the cursor. In one vector that insertion moves every
/// token after it, which made a text with many `_em_` quadratic; here the
/// tokens ahead are stored last first, so a cut near the cursor moves only the
/// few tokens between it and the cursor. Positions are still plain indices into
/// the whole sequence.
struct Tokens<'a> {
    /// The tokens before the cursor, in order.
    behind: Vec<(SyntaxKind, &'a str)>,
    /// The tokens from the cursor on, the last one first: the next is at the end.
    ahead: Vec<(SyntaxKind, &'a str)>,
}

impl<'a> Tokens<'a> {
    fn new(mut all: Vec<(SyntaxKind, &'a str)>) -> Self {
        all.reverse();
        Self {
            behind: Vec::new(),
            ahead: all,
        }
    }

    fn len(&self) -> usize {
        self.behind.len() + self.ahead.len()
    }

    fn get(&self, i: usize) -> Option<(SyntaxKind, &'a str)> {
        match i.checked_sub(self.behind.len()) {
            None => self.behind.get(i).copied(),
            Some(k) => self.ahead.len().checked_sub(k + 1).map(|j| self.ahead[j]),
        }
    }

    /// Moves the cursor past the next token.
    fn advance(&mut self) {
        if let Some(token) = self.ahead.pop() {
            self.behind.push(token);
        }
    }

    /// Replaces token `i`, which must not be behind the cursor, by `pieces`.
    fn replace(&mut self, i: usize, pieces: Vec<(SyntaxKind, &'a str)>) {
        let j = self.ahead.len() - 1 - (i - self.behind.len());
        self.ahead.splice(j..=j, pieces.into_iter().rev());
    }
}

/// What the line after a run's last read line does to it.
enum Continuation {
    /// No marker in the right column, or a blank line: the run is over.
    End,
    /// Markers and nothing else: the run goes on, with a new paragraph.
    Empty,
    /// Markers and content: the run goes on.
    Content,
}

impl<'a> CstParser<'a> {
    fn new(src: &'a str, tokens: Vec<(SyntaxKind, &'a str)>) -> Self {
        Self {
            src,
            tokens: Tokens::new(tokens),
            pos: 0,
            builder: GreenNodeBuilder::new(),
            allow_connect: true,
            in_bracket: false,
            pipe_cols: Vec::new(),
            run_ends: Vec::new(),
            markers: HashSet::new(),
        }
    }

    // --- token access -------------------------------------------------

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// The kind of token `i` as the grammar sees it: a run's marker on a later
    /// line is white space.
    fn kind_at(&self, i: usize) -> Option<SyntaxKind> {
        match self.raw_kind_at(i)? {
            K::PIPE if !self.markers.is_empty() && self.markers.contains(&self.start_of(i)) => {
                Some(K::WHITESPACE)
            }
            k => Some(k),
        }
    }

    /// The kind of token `i` as the tokenizer read it, markers included.
    fn raw_kind_at(&self, i: usize) -> Option<SyntaxKind> {
        self.tokens.get(i).map(|(k, _)| k)
    }

    fn text_at(&self, i: usize) -> &'a str {
        self.tokens.get(i).map_or("", |(_, t)| t)
    }

    fn current_kind(&self) -> Option<SyntaxKind> {
        self.kind_at(self.pos)
    }

    fn bump(&mut self) {
        if let Some((kind, text)) = self.tokens.get(self.pos) {
            self.builder.token(kind.into(), text);
            self.tokens.advance();
            self.pos += 1;
        }
    }

    /// Emits the current token as `kind`, whatever the tokenizer called it.
    fn bump_as(&mut self, kind: SyntaxKind) {
        if let Some((_, text)) = self.tokens.get(self.pos) {
            self.builder.token(kind.into(), text);
            self.tokens.advance();
            self.pos += 1;
        }
    }

    /// The byte offset in `src` at which token `i` starts (`src.len()` past
    /// the last one). Tokens are slices of `src`, so this is pointer
    /// arithmetic, not a running sum that a split would have to keep right.
    fn start_of(&self, i: usize) -> usize {
        match self.tokens.get(i) {
            Some((_, text)) => text.as_ptr() as usize - self.src.as_ptr() as usize,
            None => self.src.len(),
        }
    }

    /// Makes `byte` the boundary between two tokens, cutting the token that
    /// spans it and tokenizing each half again: the halves are what the
    /// tokenizer would have read had the text been split there to begin with.
    ///
    /// Only ever called at or after the current token, so `pos` stays valid;
    /// a byte behind the cursor is left alone.
    fn split_at(&mut self, byte: usize) {
        // The first token at or after the cursor that ends past `byte`.
        let (mut lo, mut hi) = (self.pos, self.tokens.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.token_end(self.text_at(mid)) <= byte {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let Some((_, text)) = self.tokens.get(lo) else {
            return;
        };
        let start = self.start_of(lo);
        debug_assert!(start <= byte, "split behind the cursor");
        if start >= byte {
            return;
        }
        let (head, tail) = text.split_at(byte - start);
        let mut pieces = tokenize(head);
        pieces.extend(tokenize(tail));
        self.tokens.replace(lo, pieces);
    }

    fn token_end(&self, text: &str) -> usize {
        text.as_ptr() as usize - self.src.as_ptr() as usize + text.len()
    }

    fn bump_while(&mut self, kind: SyntaxKind) {
        while self.current_kind() == Some(kind) {
            self.bump();
        }
    }

    fn skip_ws_from(&self, mut i: usize) -> usize {
        while self.kind_at(i) == Some(K::WHITESPACE) {
            i += 1;
        }
        i
    }

    fn run_len(&self, i: usize, kind: SyntaxKind) -> usize {
        let mut n = 0;
        while self.kind_at(i + n) == Some(kind) {
            n += 1;
        }
        n
    }

    /// Is the cursor the first thing on its line? Inside a `|` run the
    /// opener's `|` is part of the line's left edge, as the markers of the
    /// lines after it are: an element written after it stands as a block.
    fn at_line_start(&self) -> bool {
        let in_run = !self.pipe_cols.is_empty();
        let mut i = self.pos;
        while i > 0
            && (self.kind_at(i - 1) == Some(K::WHITESPACE)
                || (in_run && self.kind_at(i - 1) == Some(K::PIPE)))
        {
            i -= 1;
        }
        i == 0 || self.kind_at(i - 1) == Some(K::NEWLINE)
    }

    fn is_opener(kind: Option<SyntaxKind>) -> bool {
        matches!(kind, Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE))
    }

    // --- `|` runs -----------------------------------------------------------

    /// The 1-based column, in characters, at which byte `at` stands in its
    /// line -- what `Cursor::column` says for the AST parser.
    fn column_at(&self, at: usize) -> usize {
        let line_start = self.src[..at].rfind('\n').map_or(0, |i| i + 1);
        self.src[line_start..at].chars().count() + 1
    }

    /// Index of the first token from `i` that is not white space. Markers
    /// are not skipped: this reads the tokens as the tokenizer made them.
    fn skip_raw_ws(&self, mut i: usize) -> usize {
        while self.raw_kind_at(i) == Some(K::WHITESPACE) {
            i += 1;
        }
        i
    }

    /// Does the line after the `NEWLINE` at `newline` carry the markers of a
    /// run whose columns are `cols`, one for each run it is inside of?
    /// Returns the verdict and the index of the innermost marker.
    ///
    /// The same rule as the AST parser's `pipe_run_state`, except that a
    /// marker in the wrong column ends the run here instead of being an
    /// error: the CST never fails.
    fn continuation(&self, newline: usize, cols: &[usize]) -> (Continuation, Option<usize>) {
        let mut i = self.skip_raw_ws(newline + 1);
        if matches!(self.raw_kind_at(i), None | Some(K::NEWLINE)) {
            return (Continuation::End, None);
        }
        let mut innermost = None;
        for &col in cols {
            if self.raw_kind_at(i) != Some(K::PIPE) || self.column_at(self.start_of(i)) != col {
                return (Continuation::End, None);
            }
            innermost = Some(i);
            i = self.skip_raw_ws(i + 1);
        }
        if matches!(self.raw_kind_at(i), None | Some(K::NEWLINE)) {
            (Continuation::Empty, innermost)
        } else {
            (Continuation::Content, innermost)
        }
    }

    /// The run opened by the `|` at token `opener`: the index of the `NEWLINE`
    /// after its last line (the number of tokens, if it runs to the end of the
    /// text), and the index of the marker on each line after the first.
    fn scan_pipe_run(&self, opener: usize) -> (usize, Vec<usize>) {
        let mut cols = self.pipe_cols.clone();
        cols.push(self.column_at(self.start_of(opener)));
        let mut markers = Vec::new();
        let mut i = opener;
        loop {
            while !matches!(self.raw_kind_at(i), None | Some(K::NEWLINE)) {
                i += 1;
            }
            if self.raw_kind_at(i).is_none() {
                return (self.tokens.len(), markers);
            }
            match self.continuation(i, &cols) {
                (Continuation::End, _) | (_, None) => return (i, markers),
                (_, Some(marker)) => {
                    markers.push(marker);
                    i = marker + 1;
                }
            }
        }
    }

    /// Has the run being read ended before the cursor?
    fn at_run_end(&self) -> bool {
        self.run_ends
            .last()
            .is_some_and(|&end| self.start_of(self.pos) >= end)
    }

    // --- lookahead ------------------------------------------------------

    /// Index just past the group opened at `i`, or `None` if it never closes.
    fn group_end(&self, i: usize) -> Option<usize> {
        let (open, close) = match self.kind_at(i)? {
            K::L_PAREN => (K::L_PAREN, K::R_PAREN),
            K::L_BRACKET => (K::L_BRACKET, K::R_BRACKET),
            K::L_BRACE => (K::L_BRACE, K::R_BRACE),
            K::PIPE => return Some(self.scan_pipe_run(i).0),
            _ => return None,
        };
        let mut depth = 0usize;
        let mut j = i;
        while let Some(k) = self.kind_at(j) {
            if k == open {
                depth += 1;
            } else if k == close {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            j += 1;
        }
        None
    }

    /// After an element's name (index `i`): does a group, a `:` connect, or
    /// a `#(id)` follow, so that it is an element and not plain text?
    fn groups_follow(&self, i: usize) -> bool {
        let i = self.skip_ws_from(i);
        match self.kind_at(i) {
            Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE) => true,
            Some(K::HASH) => self.id_follows(i),
            Some(K::COLON) => {
                Self::is_opener(self.kind_at(i + 1)) || self.kind_at(i + 1) == Some(K::IDENT)
            }
            _ => false,
        }
    }

    /// Index just past everything an element starting at `i` (its sigil is
    /// `@name`) owns, using only bracket matching.
    fn element_end(&self, mut i: usize) -> usize {
        i += self.sigil_len_at(i).unwrap_or(2); // `@name`, or `::name`
        let mut seen = [false; 3]; // args, content, value: each at most once
        let mut seen_id = false;
        loop {
            let j = self.skip_ws_from(i);
            match self.kind_at(j) {
                // `|` is `[` without the brackets: the same slot, closed by the
                // end of its marked run.
                Some(k @ (K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE)) => {
                    let slot = &mut seen[(k == K::L_BRACKET || k == K::PIPE) as usize
                        + 2 * (k == K::L_BRACE) as usize];
                    if std::mem::replace(slot, true) {
                        return i;
                    }
                    match self.group_end(j) {
                        Some(e) => i = e,
                        None => return self.tokens.len(),
                    }
                }
                Some(K::HASH) if !seen_id && self.id_follows(j) => {
                    seen_id = true;
                    // `#` `(` ... `)`: skip past the id the same way a
                    // bracketed group is skipped, by matching the paren.
                    match self.group_end(j + 1) {
                        Some(e) => i = e,
                        None => return self.tokens.len(),
                    }
                }
                Some(K::COLON) => {
                    let mut e = j + 1;
                    if self.kind_at(e) == Some(K::IDENT) {
                        e += 1;
                    }
                    if Self::is_opener(self.kind_at(e)) {
                        match self.group_end(e) {
                            Some(e) => i = e,
                            None => return self.tokens.len(),
                        }
                    } else {
                        return i;
                    }
                }
                _ => return i,
            }
        }
    }

    /// The number of tokens in the `@name` or `::name` sigil at `i`, if one starts there.
    ///
    /// `::` is two `COLON` tokens; it is a sigil only with a name right after.
    fn sigil_len_at(&self, i: usize) -> Option<usize> {
        match self.kind_at(i) {
            Some(K::AT) if self.kind_at(i + 1) == Some(K::IDENT) => Some(2),
            Some(K::COLON)
                if self.kind_at(i + 1) == Some(K::COLON)
                    && self.kind_at(i + 2) == Some(K::IDENT) =>
            {
                Some(3)
            }
            _ => None,
        }
    }

    /// Is the `@` or `::` at `i` a block element: an element that owns the
    /// rest of its line?
    fn is_block_element_at(&self, i: usize) -> bool {
        if self.sigil_len_at(i).is_none() {
            return false;
        }
        let end = self.element_end(i);
        let mut j = end;
        while matches!(self.kind_at(j), Some(K::WHITESPACE | K::COMMENT)) {
            j += 1;
        }
        // A bare `@name` is an element only as a block; a grouped one is a
        // block only when nothing but a comment follows it on its line.
        self.kind_at(j).is_none_or(|k| k == K::NEWLINE)
    }

    /// `=`-run at `pos` that opens a section: its level.
    fn section_level_at(&self, i: usize) -> Option<usize> {
        let n = self.run_len(i, K::EQUAL);
        if n == 0 {
            return None;
        }
        match self.kind_at(i + n) {
            None
            | Some(
                K::NEWLINE
                | K::COMMENT
                | K::WHITESPACE
                | K::L_PAREN
                | K::L_BRACKET
                | K::L_BRACE
                | K::PIPE,
            ) => Some(n),
            _ => None,
        }
    }

    fn section_level_here(&self) -> Option<usize> {
        if self.current_kind() == Some(K::EQUAL) && self.at_line_start() {
            self.section_level_at(self.pos)
        } else {
            None
        }
    }

    fn is_thematic_break_at(&self, i: usize) -> bool {
        if self.run_len(i, K::MINUS) < 3 {
            return false;
        }
        let j = self.skip_ws_from(i + self.run_len(i, K::MINUS));
        matches!(
            self.kind_at(j),
            None | Some(K::NEWLINE | K::COMMENT | K::L_BRACKET)
        )
    }

    /// Does a list item start at `i`: a `-` (`-.` for an ordered one), then
    /// white space, an element's `@`, or a group opener -- the forms
    /// `list::eat_list_marker` reads. A `-` followed by anything else is prose.
    fn is_list_start_at(&self, i: usize) -> bool {
        if self.kind_at(i) != Some(K::MINUS) {
            return false;
        }
        let mut next = i + 1;
        if self.kind_at(next) == Some(K::DOT) {
            next += 1;
        }
        match self.kind_at(next) {
            Some(K::WHITESPACE | K::AT | K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE) => true,
            Some(K::HASH) => self.id_follows(next),
            _ => false,
        }
    }

    fn is_code_block_start_at(&self, i: usize) -> bool {
        self.run_len(i, K::BACKTICK) >= 3
    }

    /// Does a block other than a paragraph start at `i` (the first token of a line)?
    fn block_starts_at(&self, i: usize) -> bool {
        match self.kind_at(i) {
            Some(K::EQUAL) => self.section_level_at(i).is_some(),
            Some(K::MINUS) => self.is_thematic_break_at(i) || self.is_list_start_at(i),
            Some(K::BACKTICK) => self.is_code_block_start_at(i),
            Some(K::LT) => true,
            Some(K::AT | K::COLON) => self.is_block_element_at(i),
            _ => false,
        }
    }

    /// At a `NEWLINE` inside a paragraph: does the paragraph end here?
    fn paragraph_ends_after_newline(&self) -> bool {
        // A trailing `\` continues the line.
        let mut b = self.pos;
        while b > 0 && self.kind_at(b - 1) == Some(K::WHITESPACE) {
            b -= 1;
        }
        if b > 0 && self.kind_at(b - 1) == Some(K::TEXT_CHUNK) && self.text_at(b - 1) == "\\" {
            return false;
        }
        let i = self.skip_ws_from(self.pos + 1);
        matches!(self.kind_at(i), None | Some(K::NEWLINE)) || self.block_starts_at(i)
    }

    // --- blocks -----------------------------------------------------------

    fn parse_root(&mut self) {
        self.builder.start_node(K::ROOT.into());
        self.parse_body(0);
        self.builder.finish_node();
    }

    /// Blocks until EOF, until a section of level `<= section_level`
    /// starts, until the `|` run being read ends, or -- when
    /// [`CstParser::in_bracket`] -- until the enclosing `[content]` group's
    /// own closing `]`.
    fn parse_body(&mut self, section_level: usize) {
        while !self.is_eof() {
            if self.at_run_end() {
                break;
            }
            if self.current_kind().is_some_and(|k| k.is_trivia()) {
                self.bump();
                continue;
            }
            if self.in_bracket && self.current_kind() == Some(K::R_BRACKET) {
                break;
            }
            if self
                .section_level_here()
                .is_some_and(|l| l <= section_level)
            {
                break;
            }
            self.parse_block();
        }
    }

    fn parse_block(&mut self) {
        let Some(kind) = self.current_kind() else {
            return;
        };
        match kind {
            K::EQUAL if self.section_level_here().is_some() => self.parse_section(),
            K::MINUS if self.at_line_start() && self.is_thematic_break_at(self.pos) => {
                self.parse_thematic_break()
            }
            K::MINUS if self.is_list_start_at(self.pos) => self.parse_list(),
            K::BACKTICK if self.is_code_block_start_at(self.pos) => self.parse_code_block(),
            K::AT | K::COLON if self.is_block_element_at(self.pos) => self.parse_block_element(),
            K::LT => self.parse_block_element(),
            _ => self.parse_paragraph(),
        }
    }

    fn parse_section(&mut self) {
        let level = self.section_level_here().unwrap_or(1);
        self.builder.start_node(K::SECTION.into());
        self.builder.start_node(K::SECTION_HEADING.into());
        self.bump_while(K::EQUAL);
        self.bump_while(K::WHITESPACE);
        if Self::is_opener(self.current_kind()) {
            self.parse_groups(false);
            // Decorative trailing `=`, which may itself be followed by groups.
            loop {
                let j = self.skip_ws_from(self.pos);
                if self.kind_at(j) != Some(K::EQUAL) {
                    break;
                }
                self.bump_while(K::WHITESPACE);
                self.bump_while(K::EQUAL);
                self.parse_groups(false);
            }
        }
        self.parse_inline(InlineStop::Line);
        self.builder.finish_node();
        self.parse_body(level);
        self.builder.finish_node();
    }

    fn parse_thematic_break(&mut self) {
        self.builder.start_node(K::THEMATIC_BREAK.into());
        self.bump_while(K::MINUS);
        self.bump_while(K::WHITESPACE);
        if self.current_kind() == Some(K::L_BRACKET) {
            self.parse_content_group();
        }
        self.parse_inline(InlineStop::Line);
        self.builder.finish_node();
    }

    fn parse_block_element(&mut self) {
        self.builder.start_node(K::BLOCK_ELEMENT.into());
        self.parse_sigil();
        self.parse_groups(true);
        while matches!(self.current_kind(), Some(K::WHITESPACE | K::COMMENT)) {
            self.bump();
        }
        self.builder.finish_node();
    }

    fn parse_list(&mut self) {
        self.builder.start_node(K::LIST.into());
        while self.is_list_start_at(self.pos) {
            self.parse_list_item();
            // In a `|` run the next item stands after the run's marker.
            let j = self.skip_ws_from(self.pos);
            if j > self.pos
                && self.is_list_start_at(j)
                && (self.pos..j).any(|i| self.raw_kind_at(i) == Some(K::PIPE))
            {
                while self.pos < j {
                    self.bump();
                }
            }
        }
        self.builder.finish_node();
    }

    fn parse_list_item(&mut self) {
        self.builder.start_node(K::LIST_ITEM.into());
        self.bump(); // '-'
        if self.current_kind() == Some(K::DOT) {
            self.bump();
        }
        self.bump_while(K::WHITESPACE);
        // The item's own groups. Reading them here keeps a bracketed item,
        // which may span lines, inside one node. `allow_connect` is left
        // as whatever the caller already had (true unless we are nested
        // inside something that disabled it), so a full-form item's own
        // trailing `:name(...)`/`:(...)`/`:{...}` -- immediately after
        // its closing bracket, nothing else there to claim it -- attaches
        // to the item itself, the same as any other sigil's groups do.
        self.parse_groups(false);
        // The rest of the line -- plain trailing text for a full-form
        // item, or the whole bracket-less sugar body -- must not let an
        // inner element (`@link` in `- @link(ref:x) :{ id: b }`) steal a
        // colon connect meant for the item: `connect_joins_the_element`
        // depends on this staying `false` here.
        let outer = std::mem::replace(&mut self.allow_connect, false);
        self.parse_inline(InlineStop::Line);
        self.allow_connect = outer;
        self.builder.finish_node();
    }

    fn parse_code_block(&mut self) {
        self.builder.start_node(K::CODE_BLOCK.into());

        let mut count = 0;
        while self.current_kind() == Some(K::BACKTICK) {
            self.bump();
            count += 1;
        }

        // Consume until matching backticks or EOF
        let mut consecutive_backticks = 0;
        while let Some(k) = self.current_kind() {
            if k == K::BACKTICK {
                consecutive_backticks += 1;
                self.bump();
                if consecutive_backticks == count {
                    break;
                }
            } else {
                consecutive_backticks = 0;
                self.bump();
            }
        }

        self.builder.finish_node();
    }

    fn parse_paragraph(&mut self) {
        self.builder.start_node(K::PARAGRAPH.into());
        self.parse_inline(InlineStop::Paragraph);
        self.builder.finish_node();
    }

    // --- inline -------------------------------------------------------------

    fn parse_inline(&mut self, stop: InlineStop) {
        let mut depth = 0usize;
        while let Some(k) = self.current_kind() {
            // The text of a `|` run stops where the run does.
            if self.at_run_end() {
                break;
            }
            match stop {
                InlineStop::Line if k == K::NEWLINE => {
                    self.bump();
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
                    // A blank line ends the span unclosed, as it does in the
                    // AST parser; so does the `]` of the group it stands in.
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
            if !(self.try_parse_code_span()
                || self.try_parse_autolink(stop)
                || self.try_parse_inline_element()
                || self.try_parse_delimited())
            {
                self.bump();
            }
        }
    }

    /// After the `NEWLINE` at `i`: is the next line empty (or the text over)?
    fn blank_line_follows(&self, i: usize) -> bool {
        let j = self.skip_ws_from(i + 1);
        matches!(self.kind_at(j), None | Some(K::NEWLINE))
    }

    /// Does the closing `delim` of the span being read start at the current
    /// token? If it starts later inside it (`_em_s` is one identifier), the
    /// token is cut there and the answer is no, for now: the next token is
    /// the closer.
    fn closer_here(&mut self, delim: &str) -> bool {
        let start = self.start_of(self.pos);
        let text = self.text_at(self.pos);
        for (off, _) in text.char_indices() {
            let at = start + off;
            if self.closes_at(at, delim) {
                if off == 0 {
                    return true;
                }
                self.split_at(at);
                return false;
            }
        }
        false
    }

    /// A closing delimiter stands at `at`: `delim` there, and not right after
    /// a space. Same rule as the AST parser's `Stop::Delim`.
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
            if !delimited_probe(&cur, delim) {
                continue;
            }
            let kind = match name {
                "strong" => K::STRONG,
                "strikeout" => K::STRIKEOUT,
                _ => K::EMPHASIS,
            };
            self.builder.start_node(kind.into());
            self.bump_delimiter(delim);
            self.parse_inline(InlineStop::Delim(delim));
            if self.closes_at(self.start_of(self.pos), delim) {
                self.bump_delimiter(delim);
            }
            self.builder.finish_node();
            return true;
        }
        false
    }

    /// Emits the delimiter at the cursor one character per token, each as the
    /// token it is, however the tokenizer read it (`__` is part of an `IDENT`).
    fn bump_delimiter(&mut self, delim: &str) {
        for c in delim.chars() {
            let at = self.start_of(self.pos);
            self.split_at(at + c.len_utf8());
            self.bump_as(match c {
                '*' => K::STAR,
                '_' => K::UNDERSCORE,
                '~' => K::TILDE,
                _ => K::TEXT_CHUNK,
            });
        }
    }

    /// `` `code` `` at the cursor: a run of backticks, then the first run of
    /// the same length on the same line. What stands between is code, read as
    /// nothing else -- the tokenizer's idea of a comment or a string included.
    fn try_parse_code_span(&mut self) -> bool {
        if self.current_kind() != Some(K::BACKTICK) {
            return false;
        }
        let bytes = self.src.as_bytes();
        let start = self.start_of(self.pos);
        let fence = bytes[start..].iter().take_while(|&&b| b == b'`').count();
        let inner_start = start + fence;
        let mut i = inner_start;
        let mut inner_end = None;
        while i < bytes.len() {
            match bytes[i] {
                b'\n' | b'\r' => break,
                b'`' => {
                    let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
                    if run == fence {
                        inner_end = Some(i);
                        break;
                    }
                    i += run;
                }
                _ => i += 1,
            }
        }
        let Some(inner_end) = inner_end else {
            return false;
        };
        self.split_at(inner_end);
        self.split_at(inner_end + fence);
        self.builder.start_node(K::CODE_SPAN.into());
        while self.start_of(self.pos) < inner_start {
            self.bump_as(K::BACKTICK);
        }
        while self.start_of(self.pos) < inner_end {
            self.bump_as(K::TEXT_CHUNK);
        }
        while self.start_of(self.pos) < inner_end + fence && !self.is_eof() {
            self.bump_as(K::BACKTICK);
        }
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
        self.split_at(end);
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

    /// Parses an element at the cursor if the tokens form one.
    fn try_parse_inline_element(&mut self) -> bool {
        let p = self.pos;
        let is_element = match self.current_kind() {
            Some(K::AT) => self.kind_at(p + 1) == Some(K::IDENT) && self.groups_follow(p + 2),
            Some(K::DOLLAR) => {
                self.kind_at(p + 1) == Some(K::L_BRACE)
                    || (self.kind_at(p + 1) == Some(K::IDENT)
                        && self.kind_at(p + 2) == Some(K::L_PAREN))
            }
            _ => false,
        };
        if !is_element {
            return false;
        }
        self.builder.start_node(K::INLINE_ELEMENT.into());
        let dollar = self.current_kind() == Some(K::DOLLAR);
        self.parse_sigil();
        if dollar {
            match self.current_kind() {
                Some(K::L_BRACE) => self.parse_interp_expr(),
                Some(K::L_PAREN) => self.parse_args_group(),
                _ => {}
            }
        } else {
            self.parse_groups(false);
        }
        self.builder.finish_node();
        true
    }

    // --- elements ---------------------------------------------------------

    fn parse_sigil(&mut self) {
        self.builder.start_node(K::SIGIL.into());
        match self.current_kind() {
            Some(K::LT) => {
                self.bump();
                while let Some(k) = self.current_kind() {
                    if matches!(k, K::GT | K::NEWLINE) {
                        break;
                    }
                    self.bump();
                }
                if self.current_kind() == Some(K::GT) {
                    self.bump();
                }
            }
            // `::name`
            Some(K::COLON) if self.sigil_len_at(self.pos) == Some(3) => {
                self.bump();
                self.bump();
                self.bump();
            }
            Some(_) => {
                self.bump();
                if self.current_kind() == Some(K::IDENT) {
                    self.bump();
                }
            }
            None => {}
        }
        self.builder.finish_node();
    }

    /// An element's `(args)`, `{value}` and `[content]` (at most one of each,
    /// in any order), then its `#(id)` slot, then its `:name(...)` connects.
    ///
    /// No ordering is enforced between `#(id)` and the other three groups,
    /// mirroring the AST-path parser's `parse_groups` -- only "before any
    /// connect" matters, and that needs no explicit check here either: a
    /// connect's own id check (in [`parse_connect`]) claims an adjacent
    /// `#(...)` for itself first, so a `#(id)` written after a connect
    /// never reaches this loop's own `id` slot at all.
    ///
    /// `|` is `[` without the brackets: the same slot, closed by the end of its
    /// run. A block element (`cross_newline`) may open it on the line after
    /// its own, `@quote(A)` and then `| text`; any other construct opens it
    /// on its own line or not at all.
    fn parse_groups(&mut self, cross_newline: bool) {
        let (mut args, mut value, mut content, mut id) = (false, false, false, false);
        loop {
            let j = self.skip_ws_from(self.pos);
            match self.kind_at(j) {
                Some(K::PIPE) if !content => {
                    content = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_pipe_content();
                }
                Some(K::NEWLINE) if cross_newline && !content && self.pipe_follows_line(j) => {
                    content = true;
                    self.bump_while(K::WHITESPACE);
                    self.bump(); // the newline
                    self.bump_while(K::WHITESPACE); // an outer run's markers
                    self.parse_pipe_content();
                }
                Some(K::L_PAREN) if !args => {
                    args = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_args_group();
                }
                Some(K::L_BRACE) if !value => {
                    value = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_value_group();
                }
                Some(K::L_BRACKET) if !content => {
                    content = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_content_group();
                }
                Some(K::HASH) if !id && self.id_follows(j) => {
                    id = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_id_group();
                }
                Some(K::COLON) if self.allow_connect && self.connect_follows(j) => {
                    self.bump_while(K::WHITESPACE);
                    self.parse_connect();
                }
                _ => break,
            }
        }
    }

    /// After the `NEWLINE` at `newline`: does the next line open a `|` run
    /// of its own (once the markers of the run it stands in are passed)?
    fn pipe_follows_line(&self, newline: usize) -> bool {
        let inside = self
            .run_ends
            .last()
            .is_none_or(|&end| self.start_of(newline) < end);
        inside && self.kind_at(self.skip_ws_from(newline + 1)) == Some(K::PIPE)
    }

    /// A `|` run, read as the `[content]` of whatever owns it: the `|` that
    /// opens it, then blocks until the run ends. The `|` that begins each
    /// later line is the run's margin, not content; the grammar does not see
    /// it (see [`CstParser::markers`]) and the tree keeps it as a `PIPE`.
    fn parse_pipe_content(&mut self) {
        let (end, markers) = self.scan_pipe_run(self.pos);
        let column = self.column_at(self.start_of(self.pos));
        let marker_starts: Vec<usize> = markers.iter().map(|&m| self.start_of(m)).collect();
        let end_byte = self.start_of(end);

        self.builder.start_node(K::CONTENT.into());
        self.bump(); // the opening '|'
        self.markers.extend(marker_starts);
        self.pipe_cols.push(column);
        self.run_ends.push(end_byte);
        let outer_connect = std::mem::replace(&mut self.allow_connect, true);
        let outer_bracket = std::mem::replace(&mut self.in_bracket, false);
        self.bump_while(K::WHITESPACE);
        self.parse_body(0);
        self.in_bracket = outer_bracket;
        self.allow_connect = outer_connect;
        self.run_ends.pop();
        self.pipe_cols.pop();
        self.builder.finish_node();
    }

    /// Is `i` a `#` immediately followed by `(`, so that it opens the one
    /// `#(id)` slot rather than being free text?
    fn id_follows(&self, i: usize) -> bool {
        self.kind_at(i + 1) == Some(K::L_PAREN)
    }

    fn parse_id_group(&mut self) {
        self.builder.start_node(K::ID_GROUP.into());
        self.bump(); // '#'
        self.bump(); // '('
        self.parse_value(K::R_PAREN);
        if self.current_kind() == Some(K::R_PAREN) {
            self.bump();
        }
        self.builder.finish_node();
    }

    fn connect_follows(&self, colon: usize) -> bool {
        let mut e = colon + 1;
        if self.kind_at(e) == Some(K::IDENT) {
            e += 1;
        }
        Self::is_opener(self.kind_at(e))
    }

    fn parse_connect(&mut self) {
        self.builder.start_node(K::CONNECT.into());
        self.bump(); // ':'
        if self.current_kind() == Some(K::IDENT) {
            self.bump();
        }
        match self.current_kind() {
            Some(K::L_PAREN) => self.parse_args_group(),
            Some(K::L_BRACE) => self.parse_value_group(),
            Some(K::L_BRACKET) => self.parse_content_group(),
            _ => {}
        }
        // A connect's own id, if immediately adjacent -- claimed here,
        // before control returns to the outer `parse_groups` loop, the
        // same way `parse_groups` itself claims one for its own element.
        let j = self.skip_ws_from(self.pos);
        if self.kind_at(j) == Some(K::HASH) && self.id_follows(j) {
            self.bump_while(K::WHITESPACE);
            self.parse_id_group();
        }
        self.builder.finish_node();
    }

    fn parse_interp_expr(&mut self) {
        self.builder.start_node(K::INTERP_EXPR.into());
        self.bump(); // '{'
        let mut depth = 1usize;
        while let Some(k) = self.current_kind() {
            match k {
                K::L_BRACE => depth += 1,
                K::R_BRACE => {
                    depth -= 1;
                    if depth == 0 {
                        self.bump();
                        break;
                    }
                }
                _ => {}
            }
            self.bump();
        }
        self.builder.finish_node();
    }

    // --- groups and values -----------------------------------------------------

    fn parse_args_group(&mut self) {
        self.builder.start_node(K::ARGS.into());
        self.bump(); // '('
        self.parse_entries(K::R_PAREN);
        self.builder.finish_node();
    }

    fn parse_value_group(&mut self) {
        self.builder.start_node(K::VALUE_DATA.into());
        self.bump(); // '{'
        self.parse_entries(K::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_content_group(&mut self) {
        self.builder.start_node(K::CONTENT.into());
        self.bump(); // '['
        let outer_connect = std::mem::replace(&mut self.allow_connect, true);
        let outer_bracket = std::mem::replace(&mut self.in_bracket, true);
        self.parse_body(0);
        self.in_bracket = outer_bracket;
        self.allow_connect = outer_connect;
        if self.current_kind() == Some(K::R_BRACKET) {
            self.bump();
        }
        self.builder.finish_node();
    }

    fn parse_entries(&mut self, close: SyntaxKind) {
        while let Some(k) = self.current_kind() {
            if k == close {
                self.bump();
                break;
            }
            if k.is_trivia() || k == K::COMMA {
                self.bump();
            } else {
                self.parse_entry(close);
            }
        }
    }

    fn parse_entry(&mut self, close: SyntaxKind) {
        let is_map = matches!(
            self.current_kind(),
            Some(
                K::IDENT
                    | K::STRING_LITERAL
                    | K::INT_NUMBER
                    | K::TRUE_KW
                    | K::FALSE_KW
                    | K::NULL_KW
            )
        ) && self.kind_at(self.skip_ws_from(self.pos + 1)) == Some(K::COLON);
        if is_map {
            self.builder.start_node(K::MAP_ENTRY.into());
            self.bump(); // key
            self.bump_while(K::WHITESPACE);
            self.bump(); // ':'
        } else {
            self.builder.start_node(K::SEQ_ITEM.into());
        }
        self.parse_value(close);
        self.builder.finish_node();
    }

    /// One value, ending before a `,`, a newline or the enclosing `close`.
    fn parse_value(&mut self, close: SyntaxKind) {
        // `key: | prose`: a `|` run as the value. It ends where its markers
        // stop, which is what keeps a `,` or a `}` in the prose from closing
        // the entry.
        self.bump_while(K::WHITESPACE);
        if self.current_kind() == Some(K::PIPE) {
            self.parse_pipe_content();
        }
        while let Some(k) = self.current_kind() {
            if k == close || matches!(k, K::COMMA | K::NEWLINE) {
                break;
            }
            match k {
                K::L_BRACE => {
                    self.builder.start_node(K::MAP_LITERAL.into());
                    self.bump();
                    self.parse_entries(K::R_BRACE);
                    self.builder.finish_node();
                }
                K::L_BRACKET => {
                    self.builder.start_node(K::SEQ_LITERAL.into());
                    self.bump();
                    self.parse_entries(K::R_BRACKET);
                    self.builder.finish_node();
                }
                K::L_PAREN => self.parse_args_group(),
                _ => {
                    if !self.try_parse_inline_element() {
                        self.bump();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kinds of `node`'s child nodes, in order.
    fn child_kinds(node: &SyntaxNode) -> Vec<SyntaxKind> {
        node.children().map(|n| n.kind()).collect()
    }

    fn find(root: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
        root.descendants().find(|n| n.kind() == kind)
    }

    fn count(root: &SyntaxNode, kind: SyntaxKind) -> usize {
        root.descendants().filter(|n| n.kind() == kind).count()
    }

    #[test]
    fn test_cst_lossless_roundtrip_all_syntaxes() {
        let cases = [
            "#[ Heading ]\n\nParagraph text with *em* and @tag[ content ].\n",
            "::config{ format: json }\n",
            "- item 1\n- item 2\n-. ordered 1\n\n```rust\nfn main() {}\n```\n",
            "// Comment line\n@card(id: 123){ priority: high }[ Note ]\n",
            "=[ A ]=\n== B ==\n=\n---\n---[ x ]---\n\ntext ${a.b} $f(x) @l[t](ref:y):rule(a)\n",
            // Unterminated everything: still lossless.
            "@a(b: {c: [1, 2\n=[ open\n${ x\n@f[ never closed",
            "",
            "\n\n  \n",
        ];

        for case in cases {
            let root = parse_cst(case);
            let roundtripped = root.text().to_string();
            assert_eq!(roundtripped, case);
        }
    }

    #[test]
    fn test_cst_node_hierarchy() {
        let src = "=[ Title ]\n\nParagraph\n";
        let root = parse_cst(src);
        assert_eq!(root.kind(), K::ROOT);

        let children: Vec<_> = root.children().map(|n| n.kind()).collect();
        assert!(children.contains(&K::SECTION));
    }

    #[test]
    fn old_hash_heading_is_plain_paragraph() {
        // `#` is free: a legacy ATX-style heading marker in the old
        // `document.rs` pipeline, now plain text here too.
        let root = parse_cst("#[ Title ]\n\nParagraph\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH, K::PARAGRAPH]);
    }

    #[test]
    fn sections_nest_by_level() {
        let src = "=[ A ]\n\ntext\n\n==[ A1 ]\n\ninner\n\n==[ A2 ]\n\n=[ B ]\n\nlast\n";
        let root = parse_cst(src);
        assert_eq!(child_kinds(&root), [K::SECTION, K::SECTION]);
        let a = root.children().next().unwrap();
        assert_eq!(
            child_kinds(&a),
            [K::SECTION_HEADING, K::PARAGRAPH, K::SECTION, K::SECTION]
        );
        let a1 = a.children().nth(2).unwrap();
        assert_eq!(child_kinds(&a1), [K::SECTION_HEADING, K::PARAGRAPH]);
        // 3 real bodies ("text"/"inner"/"last") + 4 section titles ("A"/
        // "A1"/"A2"/"B"), each title's own `[...]` now wrapping its bare
        // inline content in a `PARAGRAPH` the same way any other content
        // group does -- see `parse_content_group`'s doc comment.
        assert_eq!(count(&root, K::PARAGRAPH), 7);
    }

    #[test]
    fn section_forms() {
        // Bracketed, sugar, decorative `=`, and heading-less.
        for src in [
            "=[ T ]\n",
            "= T\n",
            "=[ T ]=\n",
            "== T ==\n",
            "=\n",
            "== // c\n",
        ] {
            let root = parse_cst(src);
            assert_eq!(child_kinds(&root), [K::SECTION], "{src:?}");
            assert_eq!(root.text().to_string(), src);
        }
        // `==mark==` is inline emphasis, not a section.
        let root = parse_cst("==mark== text\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        // A bracketed title is CONTENT inside the heading.
        let heading = find(&parse_cst("=[ T ]\n"), K::SECTION_HEADING).unwrap();
        assert_eq!(child_kinds(&heading), [K::CONTENT]);
    }

    #[test]
    fn thematic_breaks() {
        let root = parse_cst("---\n---[ Title ]---\n- item\n");
        assert_eq!(
            child_kinds(&root),
            [K::THEMATIC_BREAK, K::THEMATIC_BREAK, K::LIST]
        );
    }

    #[test]
    fn inline_elements_are_structured() {
        let src = "See @link[label](ref:x) and @ftn(1) and ${a.b} and $sum(1, 2).\n";
        let root = parse_cst(src);
        let para = find(&root, K::PARAGRAPH).unwrap();
        let elements: Vec<_> = para
            .children()
            .filter(|n| n.kind() == K::INLINE_ELEMENT)
            .collect();
        assert_eq!(elements.len(), 4);

        assert_eq!(child_kinds(&elements[0]), [K::SIGIL, K::CONTENT, K::ARGS]);
        assert_eq!(elements[0].text().to_string(), "@link[label](ref:x)");
        assert_eq!(child_kinds(&elements[1]), [K::SIGIL, K::ARGS]);
        assert_eq!(elements[1].text().to_string(), "@ftn(1)");
        assert_eq!(child_kinds(&elements[2]), [K::SIGIL, K::INTERP_EXPR]);
        assert_eq!(elements[2].text().to_string(), "${a.b}");
        assert_eq!(child_kinds(&elements[3]), [K::SIGIL, K::ARGS]);

        // `ref:x` is a structured entry.
        let entry = find(&elements[0], K::MAP_ENTRY).unwrap();
        assert_eq!(entry.text().to_string(), "ref:x");
    }

    #[test]
    fn plain_at_and_dollar_are_text() {
        for src in [
            "mail me @ home\n",
            "a@b.com\n",
            "x ^ y\n",
            "cost $5\n",
            "@name\n\n",
        ] {
            let root = parse_cst(src);
            assert_eq!(count(&root, K::INLINE_ELEMENT), 0, "{src:?}");
        }
    }

    #[test]
    fn inline_elements_nest_in_content() {
        let root = parse_cst("@note[ see @link[x](ref:y) ]{ k: v }\n");
        let el = find(&root, K::BLOCK_ELEMENT).unwrap();
        assert_eq!(child_kinds(&el), [K::SIGIL, K::CONTENT, K::VALUE_DATA]);
        let content = find(&el, K::CONTENT).unwrap();
        assert_eq!(count(&content, K::INLINE_ELEMENT), 1);
    }

    #[test]
    fn block_level_constructs_nest_in_content_with_their_own_id() {
        // `[content]` recurses into the same block grammar ROOT/SECTION
        // use, not just inline text -- a section (or list, code block)
        // nested inside it is a real `SECTION`/`ID_GROUP`, not flattened
        // into `CONTENT`'s plain text. This is what lets
        // `tomet-semantics-validator::collect_ids_cst` (the LSP's only
        // duplicate-id check) see a duplicate id nested this way.
        let root = parse_cst("@outer[\n=[ Heading ]#(x)\n\nbody\n]\n");
        let el = find(&root, K::BLOCK_ELEMENT).unwrap();
        let content = find(&el, K::CONTENT).unwrap();
        let section = find(&content, K::SECTION).unwrap();
        assert_eq!(child_kinds(&section), [K::SECTION_HEADING, K::PARAGRAPH]);
        let heading = find(&section, K::SECTION_HEADING).unwrap();
        let id_group = find(&heading, K::ID_GROUP).unwrap();
        assert_eq!(id_group.text().to_string(), "#(x)");
    }

    #[test]
    fn a_literal_bracket_in_content_prose_does_not_end_it_early() {
        // `array[0]` inside `[content]`: the inner `[`/`]` are plain
        // text, not recognized as a nested group (nothing precedes `[`
        // that would make it one), so the outer group's own `]` -- the
        // *third* one here -- is the one that actually closes it.
        // Depth-counted the same way the AST parser's `Stop::Paragraph`
        // boundary tracking is.
        let root = parse_cst("@a[ array[0] is nice ]\n");
        let content = find(&root, K::CONTENT).unwrap();
        assert_eq!(content.text().to_string(), "[ array[0] is nice ]");
    }

    #[test]
    fn bare_paragraph_content_spanning_lines_does_not_swallow_the_close() {
        // No blank line, no new block starting the second line -- under
        // the old inline-only `parse_content_group`, nothing stopped a
        // paragraph at `]` at all; under the block grammar, the
        // paragraph's own bracket-depth tracking has to catch it even
        // after crossing a newline.
        let root = parse_cst("@card[ hello\nworld ]\nafter\n");
        let content = find(&root, K::CONTENT).unwrap();
        assert_eq!(content.text().to_string(), "[ hello\nworld ]");
        // One paragraph inside the content group, one for "after".
        assert_eq!(count(&root, K::PARAGRAPH), 2);
    }

    #[test]
    fn block_element_needs_a_bare_line() {
        // Ends its line: a block. Text follows: part of a paragraph.
        assert_eq!(
            child_kinds(&parse_cst("@card(id: a){ p: 1 }\n")),
            [K::BLOCK_ELEMENT]
        );
        assert_eq!(child_kinds(&parse_cst("::meta\n")), [K::BLOCK_ELEMENT]);
        let root = parse_cst("@link[a](ref:b) trailing\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        assert_eq!(count(&root, K::INLINE_ELEMENT), 1);
    }

    #[test]
    fn groups_are_structured() {
        let root = parse_cst("@x(a: 1, list(p, q), \"k\": { n: [1, 2] })\n");
        let args = find(&root, K::ARGS).unwrap();
        assert_eq!(
            child_kinds(&args),
            [K::MAP_ENTRY, K::SEQ_ITEM, K::MAP_ENTRY]
        );
        // `list(p, q)` is a call: an ARGS inside the second entry.
        let item = args.children().nth(1).unwrap();
        assert_eq!(item.text().to_string(), "list(p, q)");
        assert_eq!(count(&item, K::ARGS), 1);
        assert_eq!(count(&root, K::MAP_LITERAL), 1);
        assert_eq!(count(&root, K::SEQ_LITERAL), 1);
        // `list(p, q)`, its `p` and `q`, and the two numbers.
        assert_eq!(count(&root, K::SEQ_ITEM), 5);
    }

    #[test]
    fn value_data_holds_entries() {
        let root = parse_cst("::meta{\n  _id: intro\n  tags: [a, b]\n}\n");
        let data = find(&root, K::VALUE_DATA).unwrap();
        let entries: Vec<_> = data.children().map(|n| n.text().to_string()).collect();
        assert_eq!(entries, ["_id: intro", "tags: [a, b]"]);
    }

    #[test]
    fn connect_joins_the_element() {
        let root = parse_cst("@link[x](ref:y):rule(allow: list(a))\n");
        let el = find(&root, K::BLOCK_ELEMENT).unwrap();
        assert_eq!(
            child_kinds(&el),
            [K::SIGIL, K::CONTENT, K::ARGS, K::CONNECT]
        );
        assert_eq!(
            find(&el, K::CONNECT).unwrap().text().to_string(),
            ":rule(allow: list(a))"
        );
        // A list item's own trailing group is not claimed by its element.
        let root = parse_cst("- @link(ref:x) :{ id: b }\n");
        let el = find(&root, K::INLINE_ELEMENT).unwrap();
        assert_eq!(count(&el, K::CONNECT), 0);
    }

    #[test]
    fn full_form_list_item_claims_its_own_trailing_connect() {
        // A full-form item's own groups reach `parse_groups`'s `COLON`
        // arm directly (nothing else sits between the closing `]` and
        // the `:`), so this now mirrors a plain element's own connect --
        // `parse_list_item` only disables `allow_connect` for the rest
        // of the line (the sugar/trailing-text scan), not for the item's
        // own groups.
        let root = parse_cst("- [ x ]:rule(allow: list(a))\n");
        let item = find(&root, K::LIST_ITEM).unwrap();
        assert_eq!(count(&item, K::CONNECT), 1);
        assert_eq!(
            find(&item, K::CONNECT).unwrap().text().to_string(),
            ":rule(allow: list(a))"
        );
    }

    #[test]
    fn each_group_kind_at_most_once() {
        // The second `(...)` is prose, not more args.
        let root = parse_cst("@link[a](ref:b) (see below)\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        let el = find(&root, K::INLINE_ELEMENT).unwrap();
        assert_eq!(count(&el, K::ARGS), 1);
    }

    #[test]
    fn paragraph_continuation_and_breaks() {
        // A trailing `\` keeps the paragraph going across a block-looking line.
        let root = parse_cst("one \\\n= two\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        // A block start on the next line ends the paragraph without a blank.
        let root = parse_cst("text\n=[ S ]\n- a\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH, K::SECTION]);
    }

    #[test]
    fn urls_and_apostrophes_do_not_break_groups() {
        // `//` after `:` is not a comment; `'` inside a word is not a quote.
        let root = parse_cst("@link[it's](url:https://example.com/a)\n=[ S ]\n");
        assert_eq!(child_kinds(&root), [K::BLOCK_ELEMENT, K::SECTION]);
    }

    #[test]
    fn typed_wrappers_walk_the_tree() {
        use tomet_ast::{AstNode, CstRoot};

        let src = "=[ A ]{ id: a }\n==[ B ]\n@x(\"k:1\": v, n: 2)\n";
        let root = CstRoot::cast(parse_cst(src)).unwrap();
        let a = root.sections().next().unwrap();
        assert_eq!(a.level(), 1);
        let b = a.sections().next().unwrap();
        assert_eq!(b.level(), 2);
        let id = a
            .heading()
            .unwrap()
            .value_data()
            .unwrap()
            .entries()
            .next()
            .unwrap();
        assert_eq!(
            (id.key().as_deref(), id.value_text().as_str()),
            (Some("id"), "a")
        );
        let el = b
            .syntax()
            .descendants()
            .find_map(tomet_ast::CstBlockElement::cast)
            .unwrap();
        let entries: Vec<_> = el
            .args()
            .unwrap()
            .entries()
            .map(|e| e.value_text())
            .collect();
        assert_eq!(entries, ["v", "2"]);
    }

    #[test]
    fn id_group_is_structured_and_roundtrips() {
        let src = "@memo(a: 1)[ x ]#(myid)\n";
        let root = parse_cst(src);
        assert_eq!(root.text().to_string(), src);
        let el = find(&root, K::BLOCK_ELEMENT).unwrap();
        assert_eq!(
            child_kinds(&el),
            [K::SIGIL, K::ARGS, K::CONTENT, K::ID_GROUP]
        );
        let id_group = find(&el, K::ID_GROUP).unwrap();
        assert_eq!(id_group.text().to_string(), "#(myid)");
    }

    #[test]
    fn id_group_can_come_before_its_groups() {
        let el = find(&parse_cst("@memo#(myid)(a: 1)\n"), K::BLOCK_ELEMENT).unwrap();
        assert_eq!(child_kinds(&el), [K::SIGIL, K::ID_GROUP, K::ARGS]);
    }

    #[test]
    fn a_connect_can_carry_its_own_id_group() {
        let el = find(
            &parse_cst("@memo:rule(allow: list(card))#(myid)\n"),
            K::CONNECT,
        )
        .unwrap();
        assert_eq!(child_kinds(&el), [K::ARGS, K::ID_GROUP]);
    }

    #[test]
    fn bare_hash_not_followed_by_paren_is_not_an_id_group() {
        // Same regression this session's AST-path fix covers: `#` must be
        // immediately followed by `(` to be the id slot, elsewhere it's
        // free text -- confirmed here for the lossless tree too.
        let root = parse_cst("::meta{a: 1}\n#[ Title ]\n");
        assert_eq!(count(&root, K::ID_GROUP), 0);
        assert_eq!(child_kinds(&root), [K::BLOCK_ELEMENT, K::PARAGRAPH]);
    }

    #[test]
    fn a_document_declaration_is_a_block_element_with_a_double_colon_sigil() {
        let src = "::kind(config)\n::meta{ id: x }\n\n=[ T ]\n";
        let root = parse_cst(src);
        assert_eq!(root.text().to_string(), src);
        assert_eq!(
            child_kinds(&root),
            [K::BLOCK_ELEMENT, K::BLOCK_ELEMENT, K::SECTION]
        );
        let kind = find(&root, K::BLOCK_ELEMENT).unwrap();
        assert_eq!(child_kinds(&kind), [K::SIGIL, K::ARGS]);
        let sigil = find(&kind, K::SIGIL).unwrap();
        assert_eq!(sigil.text().to_string(), "::kind");
        let tokens: Vec<_> = sigil
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .map(|t| t.kind())
            .collect();
        assert_eq!(tokens, [K::COLON, K::COLON, K::IDENT]);
    }

    #[test]
    fn a_declaration_takes_groups_and_connects_like_an_at_element() {
        let src = "::x(a)[ b ]{ c: 1 }#(i):rule(allow:list(k))\n";
        let root = parse_cst(src);
        assert_eq!(root.text().to_string(), src);
        let el = find(&root, K::BLOCK_ELEMENT).unwrap();
        assert_eq!(
            child_kinds(&el),
            [
                K::SIGIL,
                K::ARGS,
                K::CONTENT,
                K::VALUE_DATA,
                K::ID_GROUP,
                K::CONNECT
            ]
        );
    }

    #[test]
    fn the_next_declaration_is_not_a_connect_of_the_one_before() {
        let root = parse_cst("::kind(config)\n::meta{ id: x }\n");
        assert_eq!(count(&root, K::CONNECT), 0);
        assert_eq!(count(&root, K::BLOCK_ELEMENT), 2);
    }

    #[test]
    fn a_double_colon_that_starts_no_declaration_is_text() {
        for src in [
            "std::vec is a type\n",
            "text ::kind(x) in the middle\n",
            "::\n",
            "::(x)\n",
            ":::kind(x)\n",
        ] {
            let root = parse_cst(src);
            assert_eq!(root.text().to_string(), src);
            assert_eq!(count(&root, K::BLOCK_ELEMENT), 0, "{src:?}");
            assert_eq!(count(&root, K::INLINE_ELEMENT), 0, "{src:?}");
        }
    }
}
