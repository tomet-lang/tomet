//! Resilient, lossless Concrete Syntax Tree (CST) parser for Tomet.
//!
//! Preserves 100% of source characters including trivia (whitespace, newlines, comments).
//!
//! This is the one reader of the grammar's shape -- where a block, a form, a
//! group starts and ends. `parse_document` is this tree lowered into the AST
//! (see `lower.rs`), so a grammar change is made here once. The parser itself
//! never fails: broken input still gets a tree, and the errors a document is
//! refused with are found when the tree is lowered.
//!
//! ```text
//! ROOT
//!   SECTION                    `=[ T ]` and everything up to the next section
//!     SECTION_HEADING          of the same or a shallower level (they nest)
//!     PARAGRAPH | LIST | BLOCK_FORM | CODE_BLOCK | THEMATIC_BREAK | SECTION ...
//! ```
//!
//! A form (`BLOCK_FORM`, `INLINE_FORM`) is a `SIGIL` followed by
//! its groups -- `ARGS` `( )`, `VALUE_DATA` `{ }`, `CONTENT` `[ ]`, at most
//! one of each -- then any `CONNECT` (`::name(...)`). A form is a nexus, which stands on
//! `ARGS` or `CONTENT`; see the crate docs for the grammar of one, and for what is text.
//! `${ ... }` is an `INTERP_EXPR`. Inside `ARGS`/`VALUE_DATA` an entry is a
//! `MAP_ENTRY` (`key: value`) or a `SEQ_ITEM`, and nested groups are
//! `MAP_LITERAL`, `CONTENT` (a value written `[ ... ]` is blocks, as in the AST
//! parser; the list literal is gone) or `ARGS` -- a bare `(a, b)` is a sequence,
//! and `name(a, b)` a call.
//! `CONTENT` recurses into the same block grammar `ROOT`/`SECTION` use
//! (`SECTION`, `LIST`, `BLOCK_FORM`, `CODE_BLOCK`, `THEMATIC_BREAK`,
//! `PARAGRAPH`), bounded by the group's own closing `]` instead of EOF or
//! a shallower section -- a block nested inside `[content]` (and its own
//! `#(id)`) is represented, not flattened into inline text.
//!
//! A `CODE_BLOCK` is a fence of three backticks or more, or -- two backticks at the
//! start of a line -- a line raw: the lines that open that way, with no closing line.
//!
//! Running text carries `EMPHASIS`, `STRONG`, `STRIKEOUT`, `CODE_SPAN` and
//! `AUTOLINK` nodes where the AST has an `em`, `strong`, `strikeout`,
//! `raw` or `link` element. What opens a span, and where an address ends, is
//! decided by the same functions in `inline.rs`, so the two cannot drift.
//! The tokenizer is context-free and emits tokens finely enough that delimiters
//! and structural boundaries are already token boundaries. The parser never cuts
//! tokens; it assembles compound structures (such as string literals in value
//! contexts and compound identifiers) where the grammar requires them.
//!
//! A `|` run is `[content]` spelled without brackets, and so it is the same
//! node: a `CONTENT` that opens with a `PIPE` and has no closing `]`. It is a
//! form's content (`@quote| text`, or `|` on the line after the form), a
//! list item's (`-| text`), a heading's (`=| text`) or a `key: | prose` value.
//! It ends at the first line that does not repeat its marker in the same column
//! -- a blank line, or a `|` somewhere else, ends it too -- and the error for that
//! last case (a marker in a column that continues nothing) is lowering's, not the tree's. The `|` that begins each later
//! line is a `PIPE` token among the content's own, but the grammar reads it as
//! white space, so the blocks of a run are the blocks the bracketed form has.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use tomet_cst::{GreenNodeBuilder, SyntaxKind, SyntaxNode};
use tomet_lexer::{Cursor, tokenize};

use crate::inline::{
    DELIMITERS, DelimiterIndex, autolink_end, autolink_scheme_at, delimited_probe, escaped_run_len,
    is_boundary,
};
use crate::tail::TailScan;

use SyntaxKind as K;

/// Parse `src` into a lossless, resilient [`SyntaxNode`].
///
/// Guaranteed Invariant: `parse_cst(src).text().to_string() == src`.
pub fn parse_cst(src: &str) -> SyntaxNode {
    let tokens = tokenize(src);
    let mut parser = CstParser::new(src, &tokens);
    parser.parse_root();
    SyntaxNode::new_root(parser.builder.finish())
}

/// [`parse_cst`] for a source that is one value, as [`parse_cst_value`] reads
/// it: the entries of its map (or its one value) under a `ROOT`, with the
/// forms, `[content]` and `|` runs they embed as nodes.
pub fn parse_cst_value(src: &str) -> SyntaxNode {
    let tokens = tokenize(src);
    let mut parser = CstParser::new(src, &tokens);
    parser.builder.start_node(K::ROOT.into());
    // No closer: `ROOT` is never a token, so the entries run to the end.
    parser.parse_entries(K::ROOT);
    parser.builder.finish_node();
    SyntaxNode::new_root(parser.builder.finish())
}

/// How deep constructs may nest: a `[content]` in a form in a `[content]`, a group in a group, an
/// emphasis in an emphasis, a list under a list, a section under a section. Past it the parser stops
/// reading structure and keeps the text flat in a `TOO_DEEP` node, so no recursion here, or in what
/// reads the tree, goes deeper than this (a deeper document used to abort the process with a stack
/// overflow).
pub const MAX_DEPTH: usize = tomet_cst::MAX_DEPTH;

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineStop {
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

/// What the grammar says of the chain of groups a body's line ends in, from one place.
#[derive(Clone, Copy)]
enum TailVerdict {
    /// They are the owner's, from this offset on: the first of the groups that are valid.
    From(usize),
    /// They read to the end of the line and none of them, counted from the end, is valid.
    Invalid,
    /// They do not read to the end of the line: it was prose that looked like a group.
    NotGroups,
}

struct CstParser<'a, 't> {
    src: &'a str,
    tokens: &'t [(SyntaxKind, &'a str)],
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    /// Whether a form may claim a `::name(...)` group that follows it.
    /// A list item's own top-level form may not: the group is the item's.
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
    /// answers `WHITESPACE` for them: the grammar folds a marker away with
    /// the newline before it. The tree still
    /// gets them as `PIPE` tokens, because [`CstParser::bump`] emits what the
    /// tokenizer read.
    markers: HashSet<usize>,
    /// How many constructs of the kinds [`MAX_DEPTH`] counts the parser is inside of.
    depth: usize,
    /// Where delimiters could close, and where paragraphs end: see [`DelimiterIndex`].
    delimiters: DelimiterIndex,
    /// Where `src` stands in the text being read: 0, but for a trial parse of a slice of it.
    base: usize,
    /// What the grammar said of the groups that begin at each offset of the text being read (see
    /// [`CstParser::tail_verdict`]). Shared with the trial parsers, whose slices it is asked about.
    tail_verdicts: Rc<RefCell<HashMap<usize, TailVerdict>>>,
    /// Token indices of all `COMMENT_BLOCK_CLOSE` (`*/`) tokens.
    block_closers: Vec<usize>,
    /// True while inside a raw/fenced code block where comments are not parsed.
    in_raw_block: bool,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum MapKey {
    String,
    Name,
    Simple,
}

impl<'a, 't> CstParser<'a, 't> {
    fn new(src: &'a str, tokens: &'t [(SyntaxKind, &'a str)]) -> Self {
        let mut block_closers = Vec::new();
        for i in 0..tokens.len().saturating_sub(1) {
            if tokens[i].0 == K::STAR && tokens[i + 1].0 == K::SLASH {
                let start_star = tokens[i].1.as_ptr() as usize;
                let start_slash = tokens[i + 1].1.as_ptr() as usize;
                if start_slash == start_star + 1 {
                    block_closers.push(i + 1);
                }
            }
        }
        Self {
            src,
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::new(),
            allow_connect: true,
            in_bracket: false,
            pipe_cols: Vec::new(),
            run_ends: Vec::new(),
            markers: HashSet::new(),
            depth: 0,
            delimiters: DelimiterIndex::new(src),
            base: 0,
            tail_verdicts: Rc::default(),
            block_closers,
            in_raw_block: false,
        }
    }

    // --- token access -------------------------------------------------

    fn is_eof(&self) -> bool {
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
    fn kind_at(&self, i: usize) -> Option<SyntaxKind> {
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
    fn raw_kind_at(&self, i: usize) -> Option<SyntaxKind> {
        self.tokens.get(i).map(|(k, _)| *k)
    }

    fn text_at(&self, i: usize) -> &'a str {
        self.tokens.get(i).map_or("", |(_, t)| *t)
    }

    fn current_kind(&self) -> Option<SyntaxKind> {
        self.kind_at(self.pos)
    }

    /// How many tokens form the comment starting at `from`.
    ///
    /// For line comments (`COMMENT_LINE_OPEN`), runs to the next newline.
    /// For block comments (`SLASH` + `STAR`), runs to the matching closer (`STAR` + `SLASH`),
    /// or if unclosed, to the end of the line (newline).
    fn comment_len_at(&self, from: usize) -> usize {
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

    fn bump(&mut self) {
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
    fn bump_as(&mut self, kind: SyntaxKind) {
        if let Some(&(_, text)) = self.tokens.get(self.pos) {
            self.builder.token(kind.into(), text);
            self.pos += 1;
        }
    }

    /// The byte offset in `src` at which token `i` starts (`src.len()` past
    /// the last one). Tokens are slices of `src`, so this is pointer
    /// arithmetic.
    fn start_of(&self, i: usize) -> usize {
        match self.tokens.get(i) {
            Some((_, text)) => text.as_ptr() as usize - self.src.as_ptr() as usize,
            None => self.tokens.last().map_or(self.src.len(), |(_, text)| {
                text.as_ptr() as usize - self.src.as_ptr() as usize + text.len()
            }),
        }
    }

    fn token_at_offset(&self, offset: usize) -> usize {
        self.tokens
            .partition_point(|(_, t)| (t.as_ptr() as usize - self.src.as_ptr() as usize) < offset)
    }

    fn name_tokens_at(&self, from: usize) -> usize {
        let Some(first_kind) = self.raw_kind_at(from) else {
            return 0;
        };
        let first_text = self.text_at(from);
        let valid_start = match first_kind {
            K::IDENT => first_text.chars().next().is_some_and(is_name_start),
            K::UNDERSCORE => true,
            K::TRUE_KW | K::FALSE_KW | K::NULL_KW => {
                let next = from + 1;
                self.start_of(next) == self.start_of(from) + first_text.len()
                    && matches!(
                        self.raw_kind_at(next),
                        Some(K::UNDERSCORE | K::MINUS | K::DOT | K::IDENT | K::INT_NUMBER)
                    )
            }
            _ => false,
        };
        if !valid_start {
            return 0;
        }
        let mut count = 1;
        let mut last_end = self.start_of(from) + first_text.len();
        while let Some(kind) = self.raw_kind_at(from + count) {
            let tok_start = self.start_of(from + count);
            if tok_start != last_end {
                break;
            }
            let tok_text = self.text_at(from + count);
            let is_cont = match kind {
                K::IDENT
                | K::INT_NUMBER
                | K::UNDERSCORE
                | K::MINUS
                | K::TRUE_KW
                | K::FALSE_KW
                | K::NULL_KW => true,
                K::DOT => {
                    let next = from + count + 1;
                    let next_start = self.start_of(next);
                    if next_start == tok_start + tok_text.len() {
                        matches!(
                            self.raw_kind_at(next),
                            Some(
                                K::IDENT
                                    | K::INT_NUMBER
                                    | K::UNDERSCORE
                                    | K::MINUS
                                    | K::TRUE_KW
                                    | K::FALSE_KW
                                    | K::NULL_KW
                            )
                        )
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if !is_cont {
                break;
            }
            last_end = tok_start + tok_text.len();
            count += 1;
        }
        count
    }

    fn name_at(&self, i: usize) -> bool {
        self.name_tokens_at(i) > 0
    }

    fn bump_name(&mut self) {
        let count = self.name_tokens_at(self.pos);
        if count == 0 {
            return;
        }
        let start = self.start_of(self.pos);
        let end = self.start_of(self.pos + count);
        self.builder.token(K::IDENT.into(), &self.src[start..end]);
        self.pos += count;
    }

    fn parse_name_node(&mut self) {
        let count = self.name_tokens_at(self.pos);
        if count == 0 {
            return;
        }
        self.builder.start_node(K::NAME.into());
        for _ in 0..count {
            self.bump();
        }
        self.builder.finish_node();
    }

    fn bump_string_literal(&mut self) {
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

    fn map_key_at(&self, from: usize) -> Option<MapKey> {
        match self.raw_kind_at(from)? {
            K::QUOTE => {
                let start = self.start_of(from);
                let bytes = self.src.as_bytes();
                if start >= bytes.len() {
                    return None;
                }
                let quote = bytes[start];
                let mut cur = start + 1;
                let mut escaped = false;
                while cur < bytes.len() {
                    let b = bytes[cur];
                    cur += 1;
                    if escaped {
                        escaped = false;
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == quote {
                        break;
                    }
                }
                let after_str_tok = self.token_at_offset(cur);
                let colon_tok = self.skip_ws_from(after_str_tok);
                if self.kind_at(colon_tok) == Some(K::COLON) {
                    Some(MapKey::String)
                } else {
                    None
                }
            }
            K::INT_NUMBER | K::FLOAT_NUMBER => {
                let colon_tok = self.skip_ws_from(from + 1);
                if self.kind_at(colon_tok) == Some(K::COLON) {
                    Some(MapKey::Simple)
                } else {
                    None
                }
            }
            K::TRUE_KW | K::FALSE_KW | K::NULL_KW => {
                let n = self.name_tokens_at(from);
                if n > 1 {
                    let colon_tok = self.skip_ws_from(from + n);
                    if self.kind_at(colon_tok) == Some(K::COLON) {
                        Some(MapKey::Name)
                    } else {
                        None
                    }
                } else {
                    let colon_tok = self.skip_ws_from(from + 1);
                    if self.kind_at(colon_tok) == Some(K::COLON) {
                        Some(MapKey::Simple)
                    } else {
                        None
                    }
                }
            }
            _ => {
                let n = self.name_tokens_at(from);
                if n > 0 {
                    let colon_tok = self.skip_ws_from(from + n);
                    if self.kind_at(colon_tok) == Some(K::COLON) {
                        Some(MapKey::Name)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
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
    /// lines after it are: a form written after it stands as a block.
    fn at_line_start(&self) -> bool {
        self.line_start_at(self.pos)
    }

    /// [`CstParser::at_line_start`] for token `i`.
    fn line_start_at(&self, i: usize) -> bool {
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

    fn is_opener(kind: Option<SyntaxKind>) -> bool {
        matches!(kind, Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE))
    }

    // --- `|` runs -----------------------------------------------------------

    /// The 1-based column, in characters, at which byte `at` stands in its
    /// line -- what `Cursor::column` says.
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
    /// A marker in the wrong column ends the run here instead of being an
    /// error: the CST never fails. Lowering refuses it
    /// (`reject_misaligned_marker`).
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
    /// text), the index of the marker on each line after the first, and whether
    /// the line after the run carries a misaligned marker.
    fn scan_pipe_run(&self, opener: usize) -> (usize, Vec<usize>, bool) {
        let mut cols = self.pipe_cols.clone();
        cols.push(self.column_at(self.start_of(opener)));
        let mut markers = Vec::new();
        let mut i = opener;
        loop {
            while !matches!(self.raw_kind_at(i), None | Some(K::NEWLINE)) {
                i += 1;
            }
            if self.raw_kind_at(i).is_none() {
                return (self.tokens.len(), markers, false);
            }
            match self.continuation(i, &cols) {
                (Continuation::End, _) | (_, None) => {
                    let mut j = self.skip_raw_ws(i + 1);
                    let mut is_misaligned = false;
                    if !matches!(self.raw_kind_at(j), None | Some(K::NEWLINE)) {
                        for &col in &cols {
                            if self.raw_kind_at(j) != Some(K::PIPE) {
                                break;
                            }
                            if self.column_at(self.start_of(j)) != col {
                                is_misaligned = true;
                                break;
                            }
                            j = self.skip_raw_ws(j + 1);
                        }
                    }
                    return (i, markers, is_misaligned);
                }
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
            if k == K::QUOTE {
                let start = self.start_of(j);
                let bytes = self.src.as_bytes();
                if start < bytes.len() {
                    let quote = bytes[start];
                    let mut cur = start + 1;
                    let mut escaped = false;
                    while cur < bytes.len() {
                        let b = bytes[cur];
                        if b == b'\n' {
                            break;
                        }
                        cur += 1;
                        if escaped {
                            escaped = false;
                        } else if b == b'\\' {
                            escaped = true;
                        } else if b == quote {
                            break;
                        }
                    }
                    j = self.token_at_offset(cur);
                    continue;
                }
            }
            if j > i
                && let Some(len) = escaped_run_len(self.src.as_bytes(), self.start_of(j))
            {
                let end = self.start_of(j) + len;
                while self.start_of(j) < end {
                    j += 1;
                }
                continue;
            }
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

    /// After a form's name (index `i`): does a group, a `:` connect, or
    /// a `#(id)` follow, so that it is a form and not plain text?
    ///
    /// Only what [`CstParser::parse_groups`] would take counts: a `::name` with no group
    /// after it is not a connect (`@alice:bob`), and a connect where none is allowed is
    /// text. A bare `@name` is prose, and a form that takes nothing would not be one.
    fn groups_follow(&self, i: usize) -> bool {
        let i = self.skip_ws_from(i);
        match self.kind_at(i) {
            Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE) => true,
            Some(K::HASH) => self.id_follows(i),
            Some(K::COLON) => self.allow_connect && self.connect_follows(i),
            _ => false,
        }
    }

    // --- boxes ------------------------------------------------------------------

    fn slot_name_tokens_at(&self, i: usize, first: impl Fn(char) -> bool) -> usize {
        let Some(first_kind) = self.raw_kind_at(i) else {
            return 0;
        };
        let first_text = self.text_at(i);
        let valid_start = match first_kind {
            K::IDENT | K::TRUE_KW | K::FALSE_KW | K::NULL_KW => {
                first_text.chars().next().is_some_and(first)
            }
            K::UNDERSCORE => first('_'),
            _ => false,
        };
        if !valid_start {
            return 0;
        }
        let mut count = 1;
        let mut last_end = self.start_of(i) + first_text.len();
        while let Some(kind) = self.raw_kind_at(i + count) {
            let tok_start = self.start_of(i + count);
            if tok_start != last_end {
                break;
            }
            let tok_text = self.text_at(i + count);
            let is_cont = match kind {
                K::IDENT
                | K::INT_NUMBER
                | K::UNDERSCORE
                | K::MINUS
                | K::TRUE_KW
                | K::FALSE_KW
                | K::NULL_KW => true,
                K::DOT => {
                    let next = i + count + 1;
                    let next_start = self.start_of(next);
                    if next_start == tok_start + tok_text.len() {
                        matches!(
                            self.raw_kind_at(next),
                            Some(
                                K::IDENT
                                    | K::INT_NUMBER
                                    | K::UNDERSCORE
                                    | K::MINUS
                                    | K::TRUE_KW
                                    | K::FALSE_KW
                                    | K::NULL_KW
                            )
                        )
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if !is_cont {
                break;
            }
            last_end = tok_start + tok_text.len();
            count += 1;
        }
        count
    }

    /// The box that starts at the `<` at `i`, if the tokens form one: whether it
    /// has a mark, and the index of its closing `>`. `None` leaves the `<` as text.
    ///
    /// A box is tight: no white space inside its head. An unmarked one is a name
    /// tag, `<` a letter-first name and `>`. A marked one is `<?`, an optional name,
    /// an optional `(args)` and an optional `{data}`, and `>`. The `>` is the first
    /// one outside those groups, which a string or a nested bracket cannot close.
    fn slot_at(&self, i: usize) -> Option<(bool, usize)> {
        if self.kind_at(i) != Some(K::LT) {
            return None;
        }
        let marked = self.kind_at(i + 1) == Some(K::QUESTION);
        let mut j = i + 1 + usize::from(marked);
        if !marked {
            let n = self.slot_name_tokens_at(j, char::is_alphabetic);
            if n == 0 {
                return None;
            }
            j += n;
            return (self.kind_at(j) == Some(K::GT)).then_some((false, j));
        }
        let n = self.slot_name_tokens_at(j, is_name_start);
        if n > 0 {
            j += n;
        }
        let (mut args, mut value) = (false, false);
        loop {
            match self.kind_at(j) {
                Some(K::L_PAREN) if !args => {
                    args = true;
                    j = self.group_end(j)?;
                }
                Some(K::L_BRACE) if !value => {
                    value = true;
                    j = self.group_end(j)?;
                }
                _ => break,
            }
        }
        (self.kind_at(j) == Some(K::GT)).then_some((true, j))
    }

    /// Index just past everything a form starting at `i` (its sigil is
    /// `@name`) owns, using only bracket matching.
    fn form_end(&self, mut i: usize) -> usize {
        i += self.sigil_len_at(i).unwrap_or(2); // `@name`, or `::name`
        let mut seen = [false; 3]; // args, content, value: each at most once
        let mut seen_id = false;
        loop {
            let mut j = self.skip_ws_from(i);
            // A comment after the form does not end it either.
            while self.kind_at(j) == Some(K::COMMENT) {
                j = self.skip_ws_from(j + self.comment_len_at(j));
            }
            // A group may stand on the line after the form's own, with no
            // blank line between: `@file(a)` and then `[ text ]`.
            if self.kind_at(j) == Some(K::NEWLINE) {
                let k = self.skip_ws_from(j + 1);
                let free = match self.kind_at(k) {
                    Some(K::L_PAREN) => !seen[0],
                    Some(K::L_BRACKET) => !seen[1],
                    Some(K::L_BRACE) => !seen[2],
                    Some(K::HASH) => !seen_id && self.id_follows(k),
                    Some(K::COLON) => self.connect_follows_on_next_line(k),
                    _ => false,
                };
                if free {
                    j = k;
                }
            }
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
                    let mut e = j + self.connect_colons(j).unwrap_or(1);
                    if self.name_at(e) {
                        e += self.name_tokens_at(e);
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
            Some(K::AT) if self.name_at(i + 1) => Some(1 + self.name_tokens_at(i + 1)),
            Some(K::COLON) if self.is_double_colon_sigil_at(i) => {
                Some(2 + self.name_tokens_at(i + 2))
            }
            _ => None,
        }
    }

    fn is_double_colon_sigil_at(&self, i: usize) -> bool {
        self.kind_at(i) == Some(K::COLON)
            && self.kind_at(i + 1) == Some(K::COLON)
            && self.name_at(i + 2)
    }

    /// Is the `::` at `i` a declaration that owns the rest of its line? (An `@` form or a box
    /// is told from a paragraph after it is read: see [`CstParser::parse_form_line`].)
    fn is_block_form_at(&self, i: usize) -> bool {
        if !self.is_double_colon_sigil_at(i) {
            return false;
        }
        let mut j = self.form_end(i);
        while let Some(k) = self.kind_at(j) {
            if k == K::WHITESPACE {
                j += 1;
            } else if k == K::COMMENT {
                j += self.comment_len_at(j);
            } else {
                break;
            }
        }
        self.kind_at(j).is_none_or(|k| k == K::NEWLINE)
    }

    /// After a line that holds nothing but a form, at its `NEWLINE`: does the next line fold
    /// into it, as running text goes on across lines? Not if that line is blank, ends the `[content]`
    /// or `|` run being read, or starts a block (`;`, a list marker, a section, ...).
    fn next_line_folds(&self, newline: usize) -> bool {
        if self.kind_at(newline) != Some(K::NEWLINE) {
            return false;
        }
        let k = self.skip_ws_from(newline + 1);
        let past_run = self
            .run_ends
            .last()
            .is_some_and(|&end| self.kind_at(k).is_some() && self.start_of(k) >= end);
        let closes = self.in_bracket && self.kind_at(k) == Some(K::R_BRACKET);
        !(past_run
            || closes
            || self.kind_at(k) == Some(K::COMMENT)
            || matches!(self.kind_at(k), None | Some(K::NEWLINE))
            || self.block_starts_at(k))
    }

    /// A line that opens with `@name` and nothing else (`after_name` is the index after the
    /// name): is it a block of its own rather than the first words of a paragraph?
    ///
    /// It is when it is the whole of its line and the next line neither opens a group that the
    /// name takes (see [`CstParser::parse_groups`]) nor folds into it
    /// ([`CstParser::next_line_folds`]).
    fn bare_name_stands_alone(&self, after_name: usize) -> bool {
        let end = self.past_blanks(after_name);
        match self.kind_at(end) {
            None => true,
            Some(K::NEWLINE) => self.group_opens_after(end) || !self.next_line_folds(end),
            Some(_) => false,
        }
    }

    /// After the `NEWLINE` at `newline`: does the next line open a group that the form above
    /// it takes across the line break? The ones [`CstParser::parse_groups`] takes there.
    fn group_opens_after(&self, newline: usize) -> bool {
        let k = self.skip_ws_from(newline + 1);
        match self.kind_at(k) {
            Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE) => true,
            Some(K::HASH) => self.id_follows(k),
            Some(K::COLON) => self.allow_connect && self.connect_follows_on_next_line(k),
            Some(K::PIPE) => self.pipe_follows_line(newline),
            _ => false,
        }
    }

    /// Index of the first token from `i` that is not white space or a comment.
    fn past_blanks(&self, mut i: usize) -> usize {
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

    /// `=`-run at `pos` that opens a section: its level.
    fn section_level_at(&self, i: usize) -> Option<usize> {
        let n = self.run_len(i, K::EQUAL);
        if n == 0 {
            return None;
        }
        match self.kind_at(i + n) {
            // `==#(id)`: a section whose heading is its id alone.
            Some(K::HASH) if self.id_follows(i + n) => Some(n),
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
    /// white space, a form's `@`, or a group opener -- the forms
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

    /// A fence (three backticks or more), or, at the start of a line, a line raw
    /// (exactly two). A single backtick is an inline code span.
    fn is_code_block_start_at(&self, i: usize) -> bool {
        match self.run_len(i, K::BACKTICK) {
            0 | 1 => false,
            2 => self.line_start_at(i),
            _ => true,
        }
    }

    /// Does a block other than a paragraph start at `i` (the first token of a line)?
    fn block_starts_at(&self, i: usize) -> bool {
        match self.kind_at(i) {
            Some(K::EQUAL) => self.section_level_at(i).is_some(),
            Some(K::MINUS) => self.is_thematic_break_at(i) || self.is_list_start_at(i),
            Some(K::BACKTICK) => self.is_code_block_start_at(i),
            // A line-start `;` breaks the block; it is no block of its own.
            Some(K::SEMI) => true,
            // So does a comment on a line of its own.
            Some(K::COMMENT) => true,
            // An `@` form is not here: a line that starts with one is
            // running text and folds into the paragraph. A `::` declaration
            // is only ever a block.
            Some(K::COLON) => self.is_block_form_at(i),
            _ => false,
        }
    }

    /// At a `NEWLINE` inside a paragraph: does the paragraph end here?
    fn paragraph_ends_after_newline(&self) -> bool {
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
            // A `;` at the start of a line only breaks the block before it: it
            // stays in the tree as a token of the body, like trivia.
            if self.current_kind() == Some(K::SEMI) && self.at_line_start() {
                self.bump();
                continue;
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
            K::AT | K::LT => self.parse_form_line(),
            K::COLON if self.is_block_form_at(self.pos) => self.parse_block_form(),
            // A declaration that does not stand alone: still read as one, with the text after it
            // marked as the mistake.
            K::COLON
                if self.pipe_cols.is_empty()
                    && self.is_double_colon_sigil_at(self.pos)
                    && self.groups_follow(self.pos + 2 + self.name_tokens_at(self.pos + 2)) =>
            {
                self.parse_declaration_with_stray_text()
            }
            _ => self.parse_paragraph(),
        }
    }

    fn parse_section(&mut self) {
        let level = self.section_level_here().unwrap_or(1);
        self.builder.start_node(K::SECTION.into());
        self.builder.start_node(K::SECTION_HEADING.into());
        self.bump_while(K::EQUAL);
        self.bump_while(K::WHITESPACE);
        let full_form = Self::is_opener(self.current_kind())
            || (self.current_kind() == Some(K::HASH) && self.id_follows(self.pos));
        // A body with a title in it: text after white space, not a group or a comment.
        let sugar = !full_form
            && self
                .src
                .as_bytes()
                .get(self.start_of(self.pos).wrapping_sub(1))
                .is_some_and(|b| *b == b' ' || *b == b'\t')
            && !matches!(self.current_kind(), None | Some(K::NEWLINE | K::COMMENT));
        if full_form {
            self.parse_groups(true, false);
            // Decorative trailing `=`, which may itself be followed by groups.
            loop {
                let j = self.skip_ws_from(self.pos);
                if self.kind_at(j) != Some(K::EQUAL) {
                    break;
                }
                self.bump_while(K::WHITESPACE);
                self.bump_while(K::EQUAL);
                self.parse_groups(false, false);
            }
        }
        if sugar {
            self.parse_sugar_tail();
            // A `::connect` on the line after the heading is the heading's own.
            let k = self.skip_ws_from(self.pos);
            if self.kind_at(k) == Some(K::COLON) && self.connect_follows(k) {
                self.bump_while(K::WHITESPACE);
                self.parse_groups(false, false);
                self.parse_inline(InlineStop::Line);
            }
        } else {
            self.parse_inline(InlineStop::Line);
        }
        self.builder.finish_node();
        if self.too_deep() {
            self.parse_too_deep_line();
        } else {
            self.depth += 1;
            self.parse_body(level);
            self.depth -= 1;
        }
        self.builder.finish_node();
    }

    /// The rest of a bracket-less body (`= text`, `- text`): the text, and the
    /// groups it ends in -- `#(id)`, `{value}`, `::connect` -- which are the owner's,
    /// not the text's.
    ///
    /// Where they begin is the leftmost place from which the groups read to the end
    /// of the line ([`CstParser::trailing_groups_start`]) and which the text before it
    /// reaches without a form of its own in the way: a `{value}` that an
    /// `@form` at the end of the text would take is the form's.
    fn parse_sugar_tail(&mut self) {
        self.bump_while(K::WHITESPACE);
        let from = self.start_of(self.pos);
        let mut line = TailScan::new(self.src, from);
        let mut resume = from;
        let mut in_body = false;
        loop {
            let Some(at) = self.trailing_groups_start(&mut line, resume) else {
                if !in_body && !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
                    self.builder.start_node(K::BODY.into());
                    in_body = true;
                }
                if in_body {
                    self.parse_inline(InlineStop::LineText);
                    self.builder.finish_node();
                }
                self.parse_inline(InlineStop::Line);
                return;
            };
            if at > self.start_of(self.pos) && !in_body {
                self.builder.start_node(K::BODY.into());
                in_body = true;
            }
            if at > self.start_of(self.pos) {
                self.parse_inline(InlineStop::Offset(at));
            }
            if self.start_of(self.pos) > at {
                // A form in the text took the groups; look for one after it.
                resume = self.start_of(self.pos);
                continue;
            }
            if in_body {
                self.builder.finish_node();
            }
            let outer = std::mem::replace(&mut self.allow_connect, true);
            self.parse_groups(false, false);
            self.allow_connect = outer;
            self.parse_inline(InlineStop::Line);
            return;
        }
    }

    /// Where the groups that end the line begin, if the line at or after `resume` has any.
    ///
    /// The first candidate whose characters make a chain of groups to the end of the line
    /// ([`TailScan`]) is read with the grammar, once, and the answer settles the rest of the line:
    /// every later candidate is inside that chain, and a candidate inside a group cannot read to
    /// the end of the line on its own.
    fn trailing_groups_start(&self, line: &mut TailScan, resume: usize) -> Option<usize> {
        let mut scan = resume;
        while let Some(at) = line.next_candidate(scan) {
            match self.tail_verdict(line, at) {
                TailVerdict::From(start) => return Some(start),
                TailVerdict::Invalid => return None,
                TailVerdict::NotGroups => scan = at + 1,
            }
        }
        None
    }

    /// What the grammar says of the chain of groups that begins at `at`, kept: a trial parse of a
    /// line asks the same of the lines inside it, and the real parse asks it again; without the
    /// answers kept, every level of nesting doubled the work.
    fn tail_verdict(&self, line: &mut TailScan, at: usize) -> TailVerdict {
        let key = self.base + at;
        if let Some(&known) = self.tail_verdicts.borrow().get(&key) {
            return known;
        }
        let verdict = self.judge_tail(line, at);
        self.tail_verdicts.borrow_mut().insert(key, verdict);
        verdict
    }

    /// The groups from `at` to the end of the line, read on a subslice of the existing
    /// token stream (without re-tokenizing) and lowered: what the AST says of them is
    /// what counts, so a `{x}` (a value group holds `key: value`) or a `#(a: 1)` is
    /// text, as it always was, and so is a trailing `[content]` or `|` run: the owner has no slot
    /// for one.
    ///
    /// The tail is the longest run of the groups, counted from the end, that is valid and claims
    /// something: more groups only add ways to be invalid, so the first group of it is found by
    /// halving, however many groups the line ends in.
    fn judge_tail(&self, line: &mut TailScan, at: usize) -> TailVerdict {
        let Some(end) = line.chain(at) else {
            return TailVerdict::NotGroups;
        };
        let tok_start = self.token_at_offset(at);
        let tok_end = self.token_at_offset(end);
        let slice = &self.src[at..end];
        let mut scratch = CstParser::new(slice, &self.tokens[tok_start..tok_end]);
        scratch.base = self.base + at;
        scratch.depth = self.depth;
        scratch.tail_verdicts = Rc::clone(&self.tail_verdicts);
        scratch.builder.start_node(K::ROOT.into());
        scratch.parse_groups(false, false);
        scratch.bump_while(K::WHITESPACE);
        scratch.builder.finish_node();
        if !scratch.is_eof() {
            return TailVerdict::NotGroups;
        }
        let root = SyntaxNode::new_root(scratch.builder.finish());
        let groups: Vec<SyntaxNode> = root.children().collect();
        let read = |first: usize| -> bool {
            let slice = &groups[first..];
            !slice.is_empty() && is_valid_tail_group_list(slice)
        };
        let (mut low, mut high) = (0, groups.len());
        while low < high {
            let mid = low + (high - low) / 2;
            if read(mid) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        match groups.get(low) {
            Some(first) if read(low) && tail_claims_something(&groups[low..]) => {
                TailVerdict::From(at + usize::from(first.text_range().start()))
            }
            _ => TailVerdict::Invalid,
        }
    }
}

fn validate_args_group(node: &SyntaxNode) -> bool {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_PAREN)
        || node.text().to_string().trim_end().ends_with(')');
    let entry_children: Vec<SyntaxNode> = node
        .children()
        .filter(|c| matches!(c.kind(), K::MAP_ENTRY | K::SEQ_ITEM | K::ERROR))
        .collect();

    if entry_children.is_empty() {
        return closed;
    }
    if !closed {
        return false;
    }

    let mut keys = Vec::new();
    for (i, child) in entry_children.iter().enumerate() {
        if child.kind() == K::ERROR {
            return false;
        }
        if child.kind() == K::MAP_ENTRY {
            let has_colon = child.children_with_tokens().any(|e| e.kind() == K::COLON);
            if has_colon {
                let key_str: String = child
                    .children_with_tokens()
                    .take_while(|e| e.kind() != K::COLON)
                    .filter(|e| !e.kind().is_trivia())
                    .map(|e| e.to_string())
                    .collect();
                if keys.contains(&key_str) {
                    return false;
                }
                keys.push(key_str);
            }
        }
        if i + 1 < entry_children.len() {
            let next_child = &entry_children[i + 1];
            let mut has_sep = false;
            let mut cur = child.last_token().and_then(|t| t.next_token());
            let next_start = next_child
                .first_token()
                .map(|t| usize::from(t.text_range().start()))
                .unwrap_or(0);
            while let Some(t) = cur {
                if usize::from(t.text_range().start()) >= next_start {
                    break;
                }
                if t.kind() == K::COMMA || t.kind() == K::NEWLINE {
                    has_sep = true;
                    break;
                }
                cur = t.next_token();
            }
            if !has_sep {
                return false;
            }
        }
    }
    true
}

fn validate_value_group(node: &SyntaxNode) -> bool {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_BRACE);
    if !closed {
        return false;
    }
    let mut keys = Vec::new();
    let entry_children: Vec<SyntaxNode> = node
        .children()
        .filter(|c| matches!(c.kind(), K::MAP_ENTRY | K::SEQ_ITEM | K::ERROR))
        .collect();

    for (i, child) in entry_children.iter().enumerate() {
        match child.kind() {
            K::ERROR => return false,
            K::MAP_ENTRY => {
                let has_colon = child.children_with_tokens().any(|e| e.kind() == K::COLON);
                if !has_colon {
                    return false;
                }
                let key_str: String = child
                    .children_with_tokens()
                    .take_while(|e| e.kind() != K::COLON)
                    .filter(|e| !e.kind().is_trivia())
                    .map(|e| e.to_string())
                    .collect();
                if keys.contains(&key_str) {
                    return false;
                }
                keys.push(key_str);
            }
            K::SEQ_ITEM => {
                let first_non_trivia = child.children_with_tokens().find(|e| !e.kind().is_trivia());
                match first_non_trivia {
                    Some(tomet_cst::SyntaxElement::Node(n))
                        if matches!(n.kind(), K::ARGS | K::INLINE_FORM | K::BLOCK_FORM) => {}
                    _ => return false,
                }
            }
            _ => return false,
        }
        if i + 1 < entry_children.len() {
            let next_child = &entry_children[i + 1];
            let mut has_sep = false;
            let mut cur = child.last_token().and_then(|t| t.next_token());
            let next_start = next_child
                .first_token()
                .map(|t| usize::from(t.text_range().start()))
                .unwrap_or(0);
            while let Some(t) = cur {
                if usize::from(t.text_range().start()) >= next_start {
                    break;
                }
                if t.kind() == K::COMMA || t.kind() == K::NEWLINE {
                    has_sep = true;
                    break;
                }
                cur = t.next_token();
            }
            if !has_sep {
                return false;
            }
        }
    }
    true
}

fn validate_id_group(node: &SyntaxNode) -> bool {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_PAREN);
    if !closed {
        return false;
    }
    let mut tokens_inside = Vec::new();
    let mut seen_lparen = false;
    for elem in node.children_with_tokens() {
        if elem.kind() == K::L_PAREN {
            seen_lparen = true;
            continue;
        }
        if elem.kind() == K::R_PAREN {
            break;
        }
        if seen_lparen {
            match elem {
                tomet_cst::SyntaxElement::Token(t) => {
                    if !t.kind().is_trivia() && t.kind() != K::COMMENT {
                        tokens_inside.push(t);
                    }
                }
                tomet_cst::SyntaxElement::Node(n) => {
                    if n.kind() == K::CONTENT {
                        return false;
                    }
                    for t in n.descendants_with_tokens().filter_map(|e| e.into_token()) {
                        if !t.kind().is_trivia() && t.kind() != K::COMMENT {
                            tokens_inside.push(t);
                        }
                    }
                }
            }
        }
    }
    if tokens_inside.len() != 1 {
        return false;
    }
    let tok = &tokens_inside[0];
    tok.kind() == K::NAME
        || tok.kind() == K::IDENT
        || tok
            .text()
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

fn validate_connect(node: &SyntaxNode) -> bool {
    if node
        .children()
        .any(|n| n.kind() == K::ID_GROUP || n.kind() == K::ERROR)
    {
        return false;
    }
    let has_name = node.children().any(|n| n.kind() == K::NAME)
        || node.children().any(|n| n.kind() == K::SIGIL);
    if !has_name {
        let mut group_count = 0;
        for child in node.children() {
            match child.kind() {
                K::ARGS => {
                    if !validate_args_group(&child) {
                        return false;
                    }
                    group_count += 1;
                }
                K::VALUE_DATA | K::MAP_LITERAL => {
                    if !validate_value_group(&child) {
                        return false;
                    }
                    group_count += 1;
                }
                _ => return false,
            }
        }
        group_count > 0
    } else {
        // Only what is structurally a connect: whether it stands on a primary group is
        // lowering's to refuse, so that a bracket-less body and a full-form head agree.
        for child in node.children() {
            match child.kind() {
                K::NAME | K::SIGIL => {}
                K::ARGS => {
                    if !validate_args_group(&child) {
                        return false;
                    }
                }
                K::VALUE_DATA | K::MAP_LITERAL => {
                    if !validate_value_group(&child) {
                        return false;
                    }
                }
                K::CONTENT => {}
                K::CONNECT => {
                    if !validate_connect(&child) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

fn is_valid_tail_group_list(groups: &[SyntaxNode]) -> bool {
    if groups.is_empty() {
        return false;
    }
    let mut seen_args = false;
    let mut seen_value = false;
    let mut seen_id = false;

    for child in groups {
        match child.kind() {
            K::SIGIL | K::SECTION_HEADING | K::NAME | K::BODY => {}
            K::LIST
            | K::INLINE_FORM
            | K::SLOT
            | K::ESCAPE
            | K::EMPHASIS
            | K::STRONG
            | K::STRIKEOUT
            | K::CODE_SPAN
            | K::AUTOLINK => {}
            K::CONTENT => return false,
            K::ERROR => return false,
            K::ARGS => {
                if seen_args || !validate_args_group(child) {
                    return false;
                }
                seen_args = true;
            }
            K::VALUE_DATA | K::MAP_LITERAL => {
                if seen_value || !validate_value_group(child) {
                    return false;
                }
                seen_value = true;
            }
            K::ID_GROUP => {
                if seen_id || !validate_id_group(child) {
                    return false;
                }
                seen_id = true;
            }
            K::CONNECT => {
                if !validate_connect(child) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn tail_claims_something(groups: &[SyntaxNode]) -> bool {
    groups.iter().any(|g| {
        matches!(
            g.kind(),
            K::ARGS | K::VALUE_DATA | K::MAP_LITERAL | K::ID_GROUP | K::CONNECT
        )
    })
}

impl<'a, 't> CstParser<'a, 't> {
    fn parse_thematic_break(&mut self) {
        self.builder.start_node(K::THEMATIC_BREAK.into());
        self.builder.start_node(K::FENCE.into());
        self.bump_while(K::MINUS);
        self.builder.finish_node();
        self.bump_while(K::WHITESPACE);
        if self.current_kind() == Some(K::L_BRACKET) {
            self.parse_content_group();
            let has_minus = {
                let j = self.skip_ws_from(self.pos);
                self.kind_at(j) == Some(K::MINUS)
            };
            let dash_count = if has_minus {
                let j = self.skip_ws_from(self.pos);
                self.run_len(j, K::MINUS)
            } else {
                0
            };
            if dash_count < 3 {
                if !has_minus {
                    let j = self.skip_ws_from(self.pos);
                    let non_ws = !matches!(self.kind_at(j), None | Some(K::NEWLINE));
                    if !non_ws {
                        self.builder.start_node(K::ERROR.into());
                        self.builder.finish_node();
                        self.bump_while(K::WHITESPACE);
                    } else {
                        self.bump_while(K::WHITESPACE);
                        self.builder.start_node(K::ERROR.into());
                        self.builder.finish_node();
                    }
                } else {
                    self.bump_while(K::WHITESPACE);
                    self.builder.start_node(K::FENCE.into());
                    self.bump_while(K::MINUS);
                    self.builder.finish_node();
                    self.builder.start_node(K::ERROR.into());
                    self.builder.finish_node();
                }
            } else {
                self.bump_while(K::WHITESPACE);
                self.builder.start_node(K::FENCE.into());
                self.bump_while(K::MINUS);
                self.builder.finish_node();
            }
            self.bump_while(K::WHITESPACE);
            if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
                self.builder.start_node(K::ERROR.into());
                self.parse_inline(InlineStop::Line);
                self.builder.finish_node();
            } else if self.current_kind() == Some(K::NEWLINE) {
                self.bump();
            }
        } else {
            self.parse_inline(InlineStop::Line);
        }
        self.builder.finish_node();
    }

    /// The head of a form that stands as a block: its sigil and groups, or a box.
    fn parse_block_head(&mut self) {
        if self.current_kind() == Some(K::LT) {
            self.parse_slot();
            return;
        }
        self.parse_sigil();
        self.parse_groups(true, true);
    }

    /// A box, at the `<` the cursor stands on ([`CstParser::slot_at`] said it is one):
    /// `<`, the mark, the name, the `(args)` and `{data}` of the head, the `>`, and
    /// for a marked one the `[content]` after it.
    fn parse_slot(&mut self) {
        self.builder.start_node(K::SLOT.into());
        let marked = self.kind_at(self.pos + 1) == Some(K::QUESTION);
        self.bump(); // '<'
        if marked {
            self.bump(); // '?'
        }
        let first_pred: fn(char) -> bool = if marked {
            is_name_start
        } else {
            char::is_alphabetic
        };
        let n = self.slot_name_tokens_at(self.pos, first_pred);
        if n > 0 {
            self.builder.start_node(K::NAME.into());
            for _ in 0..n {
                self.bump();
            }
            self.builder.finish_node();
        }
        if marked {
            let (mut args, mut value) = (false, false);
            loop {
                match self.current_kind() {
                    Some(K::L_PAREN) if !args => {
                        args = true;
                        self.parse_args_group();
                    }
                    Some(K::L_BRACE) if !value => {
                        value = true;
                        self.parse_value_group();
                    }
                    _ => break,
                }
            }
        }
        if self.current_kind() == Some(K::GT) {
            self.bump();
        }
        if marked && self.current_kind() == Some(K::L_BRACKET) {
            self.parse_content_group();
        }
        self.builder.finish_node();
    }

    /// A `::name(...)` that opens a block and has more on its line: the declaration, read as it
    /// would be alone, and the rest of the line in an `ERROR` node (a declaration stands alone).
    fn parse_declaration_with_stray_text(&mut self) {
        self.parse_block_form();
        if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
            self.builder.start_node(K::ERROR.into());
            self.parse_inline(InlineStop::Line);
            self.builder.finish_node();
        }
    }

    /// Whether one more level of nesting is past [`MAX_DEPTH`].
    fn too_deep(&self) -> bool {
        self.depth >= MAX_DEPTH
    }

    /// The rest of the current line, flat, in a `TOO_DEEP` node.
    fn parse_too_deep_line(&mut self) {
        self.builder.start_node(K::TOO_DEEP.into());
        while !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
            self.bump();
        }
        self.builder.finish_node();
    }

    /// From just after an opening bracket to just after its closer, flat, in a `TOO_DEEP` node. Strings
    /// are one token, so a bracket in one does not count.
    fn parse_too_deep_group(&mut self, open: SyntaxKind, close: SyntaxKind) {
        self.builder.start_node(K::TOO_DEEP.into());
        let mut depth = 1usize;
        while let Some(k) = self.current_kind() {
            if k == K::QUOTE {
                self.bump_string_literal();
                continue;
            }
            if k == open {
                depth += 1;
            } else if k == close {
                depth -= 1;
            }
            self.bump();
            if depth == 0 {
                break;
            }
        }
        self.builder.finish_node();
    }

    fn parse_block_form(&mut self) {
        self.builder.start_node(K::BLOCK_FORM.into());
        self.parse_block_head();
        while matches!(self.current_kind(), Some(K::WHITESPACE | K::COMMENT)) {
            self.bump();
        }
        self.builder.finish_node();
    }

    /// A line that opens with an `@` form or a box. It is read first and told what it is
    /// after: a form that is the whole of its line is a block, unless the next line folds
    /// into it (then the line is running text); a form with more after it is the start of a
    /// paragraph. Reading it first is what keeps the cost proportional to the text: deciding
    /// before, by scanning to the form's end, scans everything inside it again at every level
    /// of nesting.
    fn parse_form_line(&mut self) {
        let p = self.pos;
        let form = match self.current_kind() {
            // With a group `@name` is a form anywhere. Bare, it is a form only alone on its
            // line, and then only as a block (which lowering refuses: a nexus needs a primary
            // group): a bare `@name` that the next line folds into is running text.
            Some(K::AT) => {
                let n = self.name_tokens_at(p + 1);
                n > 0 && (self.groups_follow(p + 1 + n) || self.bare_name_stands_alone(p + 1 + n))
            }
            Some(K::LT) => matches!(self.slot_at(p), Some((true, _))),
            _ => false,
        };
        if !form {
            self.parse_paragraph();
            return;
        }
        let start = self.builder.checkpoint();
        self.parse_block_head();
        let after = self.past_blanks(self.pos);
        let whole_line = self.kind_at(after).is_none_or(|k| k == K::NEWLINE);
        if whole_line && !self.next_line_folds(after) {
            self.builder.start_node_at(start, K::BLOCK_FORM.into());
            self.bump_blanks();
            self.builder.finish_node();
            return;
        }
        // The form is the first inline of a paragraph. When it is the whole of its line, the
        // blanks after it are the form's, as they are for a block.
        self.builder.start_node_at(start, K::PARAGRAPH.into());
        self.builder.start_node_at(start, K::INLINE_FORM.into());
        if whole_line {
            self.bump_blanks();
        }
        self.builder.finish_node();
        self.parse_inline(InlineStop::Paragraph);
        self.builder.finish_node();
    }

    /// Bumps the white space and comments at the cursor.
    fn bump_blanks(&mut self) {
        while matches!(self.current_kind(), Some(K::WHITESPACE | K::COMMENT)) {
            self.bump();
        }
    }

    /// A list: items of one kind (`-` or `-.`) that stand in the same column or
    /// further out, and, inside an item, the list of the deeper marker that
    /// follows it, so `- a`, `  - b`, `- c`
    /// are one list of two items and `b` is the sublist of the first.
    fn parse_list(&mut self) {
        let ordered = self.kind_at(self.pos + 1) == Some(K::DOT);
        self.parse_list_level(ordered, 0);
    }

    /// The list marker the next line opens with, after its indentation (and a
    /// `|` run's own marker): the token index, its column, and whether it is `-.`.
    fn list_marker_ahead(&self) -> Option<(usize, usize, bool)> {
        let j = self.skip_ws_from(self.pos);
        self.is_list_start_at(j).then(|| {
            (
                j,
                self.column_at(self.start_of(j)),
                self.kind_at(j + 1) == Some(K::DOT),
            )
        })
    }

    fn parse_list_level(&mut self, ordered: bool, min_col: usize) {
        self.builder.start_node(K::LIST.into());
        while let Some((marker, col, is_ordered)) = self.list_marker_ahead() {
            if col < min_col || is_ordered != ordered {
                break;
            }
            // The indentation, and in a `|` run the marker of the line, belong
            // to the list.
            while self.pos < marker {
                self.bump();
            }
            self.builder.start_node(K::LIST_ITEM.into());
            self.parse_list_item();
            if let Some((_, deeper, deeper_ordered)) = self.list_marker_ahead()
                && deeper > col
            {
                if self.too_deep() {
                    self.parse_too_deep_line();
                } else {
                    self.depth += 1;
                    self.parse_list_level(deeper_ordered, deeper);
                    self.depth -= 1;
                }
            }
            self.builder.finish_node();
        }
        self.builder.finish_node();
    }

    fn parse_list_item(&mut self) {
        self.bump(); // '-'
        if self.current_kind() == Some(K::DOT) {
            self.bump();
        }
        self.bump_while(K::WHITESPACE);
        // The item's own groups. Reading them here keeps a bracketed item,
        // which may span lines, inside one node. `allow_connect` is left
        // as whatever the caller already had (true unless we are nested
        // inside something that disabled it), so a full-form item's own
        // trailing `::name(...)`/`::(...)`/`::{...}` -- immediately after
        // its closing bracket, nothing else there to claim it -- attaches
        // to the item itself, the same as any other sigil's groups do.
        let sugar = self.list_item_is_sugar();
        self.parse_groups(false, false);
        if sugar {
            // `- text` and `- (marker) text`: the text, and the groups it ends in.
            let outer = std::mem::replace(&mut self.allow_connect, false);
            self.parse_sugar_tail();
            self.allow_connect = outer;
        } else {
            // The rest of the line -- plain trailing text for a full-form
            // item -- must not let an inner form (`@link` in
            // `- @link(ref:x) ::{ id: b }`) steal a colon connect meant for
            // the item: `connect_joins_the_form` depends on this staying
            // `false` here.
            let outer = self.allow_connect;
            // The combine notation `-@name(...)` is the one exception: that
            // form is the item, and takes its own connects.
            let combine = self.current_kind() == Some(K::AT)
                && self.pos > 0
                && self.kind_at(self.pos - 1) != Some(K::WHITESPACE);
            if combine {
                self.allow_connect = true;
                if !self.try_parse_inline_form() {
                    self.parse_bare_combine_form();
                }
            }
            self.allow_connect = false;
            if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
                self.builder.start_node(K::BODY.into());
                self.parse_inline(InlineStop::LineText);
                self.builder.finish_node();
            }
            self.parse_inline(InlineStop::Line);
            self.allow_connect = outer;
        }
        // A `::connect` on the line after the item is the item's own, with the
        // groups that follow it on that line.
        let k = self.skip_ws_from(self.pos);
        if self.allow_connect && self.kind_at(k) == Some(K::COLON) && self.connect_follows(k) {
            self.bump_while(K::WHITESPACE);
            self.parse_groups(false, false);
            self.parse_inline(InlineStop::Line);
        }
    }

    /// Whether the item whose marker was just read is the bracket-less sugar
    /// (`- text`, `- (marker) text`): not the combine notation `-@name`, and not
    /// the full form, where a group opens on the marker (`- [ content ]`,
    /// `- (marker)[ content ]`, `- {value}`, `- #(id)`).
    fn list_item_is_sugar(&self) -> bool {
        let opens_full = |i: usize| match self.kind_at(i) {
            Some(K::L_BRACKET | K::L_BRACE | K::PIPE) => true,
            Some(K::HASH) => self.id_follows(i),
            _ => false,
        };
        let at = self.pos;
        if self.kind_at(at) == Some(K::AT) && at > 0 && self.kind_at(at - 1) != Some(K::WHITESPACE)
        {
            return false;
        }
        if opens_full(at) {
            return false;
        }
        if self.kind_at(at) == Some(K::L_PAREN) {
            return match self.group_end(at) {
                Some(end) => !opens_full(self.skip_ws_from(end).min(self.tokens.len())),
                None => true,
            };
        }
        true
    }

    /// A fenced code block: it closes on a line whose leading run of backticks
    /// is at least as long as the opening one and holds nothing else but
    /// white space. A longer opening run
    /// is how a body holds a closing-looking line; a run in the middle of a
    /// line closes nothing.
    fn parse_code_block(&mut self) {
        self.builder.start_node(K::CODE_BLOCK.into());
        let count = self.run_len(self.pos, K::BACKTICK);
        if count == 2 {
            self.parse_line_raw();
            self.builder.finish_node();
            return;
        }
        self.builder.start_node(K::FENCE.into());
        for _ in 0..count {
            self.bump();
        }
        self.builder.finish_node();
        let prev_raw = self.in_raw_block;
        self.in_raw_block = true;
        // The rest of the opening line is the info string.
        if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
            self.builder.start_node(K::INFO_STRING.into());
            while !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
                self.bump();
            }
            self.builder.finish_node();
        }
        if self.current_kind() == Some(K::NEWLINE) {
            self.bump();
        }
        while !self.is_eof() && !self.at_run_end() {
            // In a `|` run the line's own marker has been read already.
            let i = if self.pipe_cols.is_empty() {
                self.pos
            } else {
                self.skip_ws_from(self.pos)
            };
            let run = self.run_len(i, K::BACKTICK);
            if run >= count {
                let j = self.skip_ws_from(i + run);
                if matches!(self.kind_at(j), None | Some(K::NEWLINE)) {
                    while self.pos < i {
                        self.bump();
                    }
                    self.builder.start_node(K::FENCE.into());
                    while self.pos < i + run {
                        self.bump();
                    }
                    self.builder.finish_node();
                    while self.pos < j {
                        self.bump();
                    }
                    break;
                }
            }
            self.builder.start_node(K::CODE_LINE.into());
            self.bump_line();
            self.builder.finish_node();
        }
        self.in_raw_block = prev_raw;
        self.builder.finish_node();
    }

    /// A line raw: the rest of the line after two backticks is the text, and the
    /// lines under it that open with two backticks of their own are more of the
    /// same block. A blank line, or any other line, ends it.
    fn parse_line_raw(&mut self) {
        let prev_raw = self.in_raw_block;
        self.in_raw_block = true;
        let mut first = true;
        loop {
            self.builder.start_node(K::CODE_LINE.into());
            if !first {
                // The indentation and a run's markers before it.
                let next = self.skip_ws_from(self.pos);
                while self.pos < next {
                    self.bump();
                }
            }
            first = false;
            self.builder.start_node(K::FENCE.into());
            self.bump();
            self.bump();
            self.builder.finish_node();
            self.bump_line();
            self.builder.finish_node();
            if self.is_eof() || self.at_run_end() {
                break;
            }
            let next = self.skip_ws_from(self.pos);
            if self.run_len(next, K::BACKTICK) != 2 {
                break;
            }
        }
        self.in_raw_block = prev_raw;
    }

    /// Reads the rest of the line, its `NEWLINE` included.
    fn bump_line(&mut self) {
        while let Some(k) = self.current_kind() {
            self.bump();
            if k == K::NEWLINE {
                break;
            }
        }
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

    /// Parses a form at the cursor if the tokens form one.
    fn try_parse_inline_form(&mut self) -> bool {
        let p = self.pos;
        if self.current_kind() == Some(K::LT) {
            let Some((marked, _)) = self.slot_at(p) else {
                return false;
            };
            // A marked box is a form of the tree; a name tag stays running text.
            if marked {
                self.builder.start_node(K::INLINE_FORM.into());
                self.parse_slot();
                self.builder.finish_node();
            } else {
                self.parse_slot();
            }
            return true;
        }
        let is_form = match self.current_kind() {
            Some(K::AT) => {
                let n = self.name_tokens_at(p + 1);
                n > 0 && self.groups_follow(p + 1 + n)
            }
            Some(K::DOLLAR) => {
                self.kind_at(p + 1) == Some(K::L_BRACE)
                    || (self.name_at(p + 1)
                        && self.kind_at(p + 1 + self.name_tokens_at(p + 1)) == Some(K::L_PAREN))
            }
            _ => false,
        };
        if !is_form {
            return false;
        }
        self.builder.start_node(K::INLINE_FORM.into());
        let dollar = self.current_kind() == Some(K::DOLLAR);
        self.parse_sigil();
        if dollar {
            match self.current_kind() {
                Some(K::L_BRACE) => self.parse_interp_expr(),
                Some(K::L_PAREN) => self.parse_args_group(),
                _ => {}
            }
        } else {
            // Like a block form's, an inline form's groups may begin on the next line.
            self.parse_groups(true, true);
        }
        self.builder.finish_node();
        true
    }

    // --- forms -----------------------------------------------------------

    /// The `@name` of a combine item (`-@name`) that has no group after it. Anywhere else a bare
    /// `@name` is running text; here the `-` directly against the `@` says it was meant as the
    /// item's element, so the tree keeps it as a form -- a sigil and no groups -- and lowering
    /// refuses it for want of a primary group.
    fn parse_bare_combine_form(&mut self) {
        if self.current_kind() == Some(K::AT) && self.name_tokens_at(self.pos + 1) > 0 {
            self.builder.start_node(K::INLINE_FORM.into());
            self.parse_sigil();
            self.builder.finish_node();
        }
    }

    fn parse_sigil(&mut self) {
        self.builder.start_node(K::SIGIL.into());
        match self.current_kind() {
            // `::name`
            Some(K::COLON) if self.is_double_colon_sigil_at(self.pos) => {
                self.bump();
                self.bump();
                self.parse_name_node();
            }
            Some(_) => {
                self.bump();
                if self.name_at(self.pos) {
                    self.parse_name_node();
                }
            }
            None => {}
        }
        self.builder.finish_node();
    }

    /// A form's `(args)`, `{value}` and `[content]` (at most one of each,
    /// in any order), then its `#(id)` slot, then its `::name(...)` connects.
    ///
    /// No ordering is enforced between `#(id)` and the other three groups,
    /// mirroring the AST-path parser's `parse_groups` -- only "before any
    /// connect" matters, and that needs no explicit check here either: a
    /// connect's own id check (in [`parse_connect`]) claims an adjacent
    /// `#(...)` for itself first, so a `#(id)` written after a connect
    /// never reaches this loop's own `id` slot at all.
    ///
    /// `|` is `[` without the brackets: the same slot, closed by the end of its
    /// run. A block form (`cross_newline`) may open it on the line after
    /// its own, `@quote(A)` and then `| text`; any other construct opens it
    /// on its own line or not at all.
    ///
    /// With `strict`, a second group of a kind already taken, written against the last one, is
    /// an `ERROR` node (see the arm below).
    fn parse_groups(&mut self, cross_newline: bool, strict: bool) {
        let (mut args, mut value, mut content, mut id) = (false, false, false, false);
        loop {
            let mut j = self.skip_ws_from(self.pos);
            while cross_newline && self.kind_at(j) == Some(K::COMMENT) {
                j = self.skip_ws_from(j + self.comment_len_at(j));
            }
            if cross_newline && self.kind_at(j) == Some(K::NEWLINE) {
                let k = self.skip_ws_from(j + 1);
                let free = match self.kind_at(k) {
                    Some(K::L_PAREN) => !args,
                    Some(K::L_BRACKET) => !content,
                    Some(K::L_BRACE) => !value,
                    Some(K::HASH) => !id && self.id_follows(k),
                    Some(K::COLON) => self.allow_connect && self.connect_follows_on_next_line(k),
                    _ => false,
                };
                if free {
                    while self.pos < j {
                        self.bump(); // white space and comments
                    }
                    self.bump(); // the newline
                    self.bump_while(K::WHITESPACE); // an outer run's markers
                    continue;
                }
            }
            match self.kind_at(j) {
                Some(K::PIPE) if !content => {
                    content = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_pipe_content();
                }
                Some(K::NEWLINE) if cross_newline && !content && self.pipe_follows_line(j) => {
                    content = true;
                    while self.pos < j {
                        self.bump(); // white space and comments
                    }
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
                    self.parse_connect(strict);
                }
                // A group of a kind the owner has already, written against the last one. Apart
                // from it (a space, a line) it is prose; against it, it is an error, kept in the
                // tree as one and read as the group it looks like. Only a form and a
                // declaration take one of each.
                Some(k @ (K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE | K::HASH))
                    if strict
                        && j == self.pos
                        && match k {
                            K::L_PAREN => args,
                            K::L_BRACE => value,
                            K::L_BRACKET | K::PIPE => content,
                            _ => id && self.id_follows(j),
                        } =>
                {
                    self.builder.start_node(K::ERROR.into());
                    match k {
                        K::L_PAREN => self.parse_args_group(),
                        K::L_BRACE => self.parse_value_group(),
                        K::L_BRACKET => self.parse_content_group(),
                        K::PIPE => self.parse_pipe_content(),
                        _ => self.parse_id_group(),
                    }
                    self.builder.finish_node();
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
    /// it (see [`CstParser::markers`]) and the tree keeps it as a `MARGIN`.
    fn parse_pipe_content(&mut self) {
        let (end, markers, is_misaligned) = self.scan_pipe_run(self.pos);
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
        if self.too_deep() {
            self.parse_too_deep_line();
        } else {
            self.depth += 1;
            self.parse_body(0);
            self.depth -= 1;
        }
        self.in_bracket = outer_bracket;
        self.allow_connect = outer_connect;
        self.run_ends.pop();
        self.pipe_cols.pop();
        if is_misaligned {
            self.builder.start_node(K::ERROR.into());
            self.builder.finish_node();
        }
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

    /// How many `:` open the connect that starts at `colon` -- 2 for `::name(...)` and `::{...}`,
    /// 1 for the retired `:name(...)` -- if a connect starts there: a name, if there is one, and
    /// then a group.
    ///
    /// The single colon is still read as a connect, so that lowering can refuse it and say what
    /// to write; it is not text.
    fn connect_colons(&self, colon: usize) -> Option<usize> {
        if self.kind_at(colon) != Some(K::COLON) {
            return None;
        }
        let colons = if self.kind_at(colon + 1) == Some(K::COLON) {
            2
        } else {
            1
        };
        let mut e = colon + colons;
        if self.name_at(e) {
            e += self.name_tokens_at(e);
        }
        Self::is_opener(self.kind_at(e)).then_some(colons)
    }

    fn connect_follows(&self, colon: usize) -> bool {
        self.connect_colons(colon).is_some()
    }

    /// Whether the connect at `colon` may stand on the line after its owner: the retired
    /// `::name(...)`, and a nameless `::{...}` / `::(...)`. A `::name` at the start of a line is
    /// a declaration, and says so by its name.
    fn connect_follows_on_next_line(&self, colon: usize) -> bool {
        match self.connect_colons(colon) {
            Some(1) => true,
            Some(_) => !self.name_at(colon + 2),
            None => false,
        }
    }

    fn parse_connect(&mut self, strict: bool) {
        self.builder.start_node(K::CONNECT.into());
        self.bump(); // ':'
        if self.current_kind() == Some(K::COLON) {
            self.bump(); // the second ':' of '::'
        }
        if self.name_at(self.pos) {
            self.parse_name_node();
        }
        // The groups of a connect are a form's: `(args)`, `{value}` and
        // `[content]` in any order, each once -- but not a sibling `::connect`,
        // which stays for the loop this was called from.
        let outer = std::mem::replace(&mut self.allow_connect, false);
        self.parse_groups(false, strict);
        self.allow_connect = outer;
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
                K::QUOTE => {
                    self.bump_string_literal();
                    continue;
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
        if self.too_deep() {
            self.parse_too_deep_group(K::L_BRACKET, K::R_BRACKET);
            self.builder.finish_node();
            return;
        }
        self.depth += 1;
        let outer_connect = std::mem::replace(&mut self.allow_connect, true);
        let outer_bracket = std::mem::replace(&mut self.in_bracket, true);
        self.parse_body(0);
        self.in_bracket = outer_bracket;
        self.allow_connect = outer_connect;
        self.depth -= 1;
        if self.current_kind() == Some(K::R_BRACKET) {
            self.bump();
        } else {
            self.builder.start_node(K::ERROR.into());
            self.builder.finish_node();
        }
        self.builder.finish_node();
    }

    fn parse_entries(&mut self, close: SyntaxKind) {
        if self.too_deep() {
            let open = if close == K::R_PAREN {
                K::L_PAREN
            } else {
                K::L_BRACE
            };
            self.parse_too_deep_group(open, close);
            return;
        }
        self.depth += 1;
        self.parse_entries_inside(close);
        self.depth -= 1;
    }

    fn parse_entries_inside(&mut self, close: SyntaxKind) {
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
        let key = self.map_key_at(self.pos);
        if let Some(k) = key {
            self.builder.start_node(K::MAP_ENTRY.into());
            match k {
                MapKey::String => self.bump_string_literal(),
                MapKey::Name => self.bump_name(),
                MapKey::Simple => self.bump(),
            }
            self.bump_while(K::WHITESPACE);
            self.bump(); // ':'

            let mut probe = self.pos;
            while probe < self.tokens.len() && self.tokens[probe].0 == K::WHITESPACE {
                probe += 1;
            }
            if probe < self.tokens.len() && self.tokens[probe].0 == K::NEWLINE {
                let mut next_line_probe = probe + 1;
                let mut has_indent = false;
                if next_line_probe < self.tokens.len()
                    && self.tokens[next_line_probe].0 == K::WHITESPACE
                {
                    has_indent = true;
                    next_line_probe += 1;
                }
                if has_indent && next_line_probe < self.tokens.len() {
                    let k = self.tokens[next_line_probe].0;
                    if k != close && k != K::NEWLINE && self.map_key_at(next_line_probe).is_none() {
                        self.bump_while(K::WHITESPACE);
                        self.bump(); // newline
                        self.bump_while(K::WHITESPACE); // indentation
                    }
                }
            }
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
                // `[ ... ]` as a value is blocks, the same `[content]` a
                // form takes; the list literal it once was is gone. Read as
                // a list it closed at the first `]`, even one inside `` `code` ``.
                K::L_BRACKET => self.parse_content_group(),
                K::L_PAREN => self.parse_args_group(),
                // The `#(id)` of a bare entry, `(1)[text]#(a)`.
                K::HASH if self.id_follows(self.pos) => self.parse_id_group(),
                K::QUOTE => self.bump_string_literal(),
                _ => {
                    if !self.try_parse_inline_form() {
                        if self.name_at(self.pos) {
                            self.bump_name();
                        } else {
                            self.bump();
                        }
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
            "=[ A ]=\n== B ==\n=\n---\n---[ x ]---\n\ntext ${a.b} $f(x) @l[t](ref:y)::rule(a)\n",
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
        // `#` is free: it was an ATX-style heading marker once, and is plain text now.
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
    fn inline_forms_are_structured() {
        let src = "See @link[label](ref:x) and @ftn(1) and ${a.b} and $sum(1, 2).\n";
        let root = parse_cst(src);
        let para = find(&root, K::PARAGRAPH).unwrap();
        let forms: Vec<_> = para
            .children()
            .filter(|n| n.kind() == K::INLINE_FORM)
            .collect();
        assert_eq!(forms.len(), 4);

        assert_eq!(child_kinds(&forms[0]), [K::SIGIL, K::CONTENT, K::ARGS]);
        assert_eq!(forms[0].text().to_string(), "@link[label](ref:x)");
        assert_eq!(child_kinds(&forms[1]), [K::SIGIL, K::ARGS]);
        assert_eq!(forms[1].text().to_string(), "@ftn(1)");
        assert_eq!(child_kinds(&forms[2]), [K::SIGIL, K::INTERP_EXPR]);
        assert_eq!(forms[2].text().to_string(), "${a.b}");
        assert_eq!(child_kinds(&forms[3]), [K::SIGIL, K::ARGS]);

        // `ref:x` is a structured entry.
        let entry = find(&forms[0], K::MAP_ENTRY).unwrap();
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
            assert_eq!(count(&root, K::INLINE_FORM), 0, "{src:?}");
        }
    }

    #[test]
    fn inline_forms_nest_in_content() {
        let root = parse_cst("@note[ see @link[x](ref:y) ]{ k: v }\n");
        let el = find(&root, K::BLOCK_FORM).unwrap();
        assert_eq!(child_kinds(&el), [K::SIGIL, K::CONTENT, K::VALUE_DATA]);
        let content = find(&el, K::CONTENT).unwrap();
        assert_eq!(count(&content, K::INLINE_FORM), 1);
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
        let el = find(&root, K::BLOCK_FORM).unwrap();
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
        // The paragraph's `]` is found by counting depth.
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
    fn block_form_needs_a_bare_line() {
        // Ends its line: a block. Text follows: part of a paragraph.
        assert_eq!(
            child_kinds(&parse_cst("@card(id: a){ p: 1 }\n")),
            [K::BLOCK_FORM]
        );
        assert_eq!(child_kinds(&parse_cst("::meta\n")), [K::BLOCK_FORM]);
        let root = parse_cst("@link[a](ref:b) trailing\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        assert_eq!(count(&root, K::INLINE_FORM), 1);
    }

    #[test]
    fn groups_are_structured() {
        let root = parse_cst("@x(a: 1, (p, q), name(r), \"k\": { n: [1, 2] })\n");
        let args = find(&root, K::ARGS).unwrap();
        assert_eq!(
            child_kinds(&args),
            [K::MAP_ENTRY, K::SEQ_ITEM, K::SEQ_ITEM, K::MAP_ENTRY]
        );
        // `(p, q)` is a sequence, `name(r)` a call: each an ARGS inside its entry.
        let item = args.children().nth(1).unwrap();
        assert_eq!(item.text().to_string(), "(p, q)");
        assert_eq!(count(&item, K::ARGS), 1);
        let call = args.children().nth(2).unwrap();
        assert_eq!(call.text().to_string(), "name(r)");
        assert_eq!(count(&call, K::ARGS), 1);
        assert_eq!(count(&root, K::MAP_LITERAL), 1);
        // `[1, 2]` as a value is blocks: a content, not a list.
        assert_eq!(count(&root, K::CONTENT), 1);
        // `(p, q)` and its `p` and `q`, `name(r)` and its `r`.
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
    fn connect_joins_the_form() {
        let root = parse_cst("@link[x](ref:y)::rule(allow: (a))\n");
        let el = find(&root, K::BLOCK_FORM).unwrap();
        assert_eq!(
            child_kinds(&el),
            [K::SIGIL, K::CONTENT, K::ARGS, K::CONNECT]
        );
        assert_eq!(
            find(&el, K::CONNECT).unwrap().text().to_string(),
            "::rule(allow: (a))"
        );
        // A list item's own trailing group is not claimed by its form.
        let root = parse_cst("- @link(ref:x) ::{ id: b }\n");
        let el = find(&root, K::INLINE_FORM).unwrap();
        assert_eq!(count(&el, K::CONNECT), 0);
    }

    #[test]
    fn full_form_list_item_claims_its_own_trailing_connect() {
        // A full-form item's own groups reach `parse_groups`'s `COLON`
        // arm directly (nothing else sits between the closing `]` and
        // the `:`), so this now mirrors a plain form's own connect --
        // `parse_list_item` only disables `allow_connect` for the rest
        // of the line (the sugar/trailing-text scan), not for the item's
        // own groups.
        let root = parse_cst("- [ x ]::rule(allow: (a))\n");
        let item = find(&root, K::LIST_ITEM).unwrap();
        assert_eq!(count(&item, K::CONNECT), 1);
        assert_eq!(
            find(&item, K::CONNECT).unwrap().text().to_string(),
            "::rule(allow: (a))"
        );
    }

    #[test]
    fn a_connect_opens_with_two_colons() {
        let root = parse_cst("@x(a)::rule(b)::{c: 1}\n");
        let el = find(&root, K::BLOCK_FORM).unwrap();
        assert_eq!(
            child_kinds(&el),
            [K::SIGIL, K::ARGS, K::CONNECT, K::CONNECT]
        );
        let connects: Vec<_> = el
            .children()
            .filter(|n| n.kind() == K::CONNECT)
            .map(|n| n.text().to_string())
            .collect();
        assert_eq!(connects, ["::rule(b)", "::{c: 1}"]);
        // The retired single colon is still a connect in the tree, for lowering to refuse.
        let root = parse_cst("@x(a):rule(b)\n");
        let connect = find(&root, K::CONNECT).unwrap();
        assert_eq!(connect.text().to_string(), ":rule(b)");
    }

    #[test]
    fn a_double_colon_name_on_the_next_line_is_a_declaration_not_a_connect() {
        let root = parse_cst("@x(a)\n::rule(b)\n");
        assert_eq!(count(&root, K::CONNECT), 0);
        assert_eq!(child_kinds(&root), [K::BLOCK_FORM, K::BLOCK_FORM]);
        // Nameless, it cannot be one, so it is the connect of the line above.
        let root = parse_cst("@x(a)\n::{b: 1}\n");
        assert_eq!(count(&root, K::CONNECT), 1);
    }

    #[test]
    fn each_group_kind_at_most_once() {
        // The second `(...)` is prose, not more args.
        let root = parse_cst("@link[a](ref:b) (see below)\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        let el = find(&root, K::INLINE_FORM).unwrap();
        assert_eq!(count(&el, K::ARGS), 1);
    }

    #[test]
    fn paragraph_continuation_and_breaks() {
        // A block start on the next line ends the paragraph without a blank.
        let root = parse_cst("text\n=[ S ]\n- a\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH, K::SECTION]);
    }

    #[test]
    fn urls_and_apostrophes_do_not_break_groups() {
        // `//` after `:` is not a comment; `'` inside a word is not a quote.
        let root = parse_cst("@link[it's](url:https://example.com/a)\n=[ S ]\n");
        assert_eq!(child_kinds(&root), [K::BLOCK_FORM, K::SECTION]);
    }

    #[test]
    fn typed_wrappers_walk_the_tree() {
        use tomet_cst::{AstNode, CstRoot};

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
            .find_map(tomet_cst::CstBlockForm::cast)
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
        let el = find(&root, K::BLOCK_FORM).unwrap();
        assert_eq!(
            child_kinds(&el),
            [K::SIGIL, K::ARGS, K::CONTENT, K::ID_GROUP]
        );
        let id_group = find(&el, K::ID_GROUP).unwrap();
        assert_eq!(id_group.text().to_string(), "#(myid)");
    }

    #[test]
    fn id_group_can_come_before_its_groups() {
        let el = find(&parse_cst("@memo#(myid)(a: 1)\n"), K::BLOCK_FORM).unwrap();
        assert_eq!(child_kinds(&el), [K::SIGIL, K::ID_GROUP, K::ARGS]);
    }

    #[test]
    fn a_connect_can_carry_its_own_id_group() {
        let el = find(
            &parse_cst("@memo::rule(allow: (card))#(myid)\n"),
            K::CONNECT,
        )
        .unwrap();
        assert_eq!(child_kinds(&el), [K::NAME, K::ARGS, K::ID_GROUP]);
    }

    #[test]
    fn bare_hash_not_followed_by_paren_is_not_an_id_group() {
        // Same regression this session's AST-path fix covers: `#` must be
        // immediately followed by `(` to be the id slot, elsewhere it's
        // free text -- confirmed here for the lossless tree too.
        let root = parse_cst("::meta{a: 1}\n#[ Title ]\n");
        assert_eq!(count(&root, K::ID_GROUP), 0);
        assert_eq!(child_kinds(&root), [K::BLOCK_FORM, K::PARAGRAPH]);
    }

    #[test]
    fn a_document_declaration_is_a_block_form_with_a_double_colon_sigil() {
        let src = "::kind(config)\n::meta{ id: x }\n\n=[ T ]\n";
        let root = parse_cst(src);
        assert_eq!(root.text().to_string(), src);
        assert_eq!(
            child_kinds(&root),
            [K::BLOCK_FORM, K::BLOCK_FORM, K::SECTION]
        );
        let kind = find(&root, K::BLOCK_FORM).unwrap();
        assert_eq!(child_kinds(&kind), [K::SIGIL, K::ARGS]);
        let sigil = find(&kind, K::SIGIL).unwrap();
        assert_eq!(sigil.text().to_string(), "::kind");
        let tokens: Vec<_> = sigil
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .map(|t| t.kind())
            .collect();
        assert_eq!(tokens, [K::COLON, K::COLON]);
        assert_eq!(child_kinds(&sigil), [K::NAME]);
    }

    #[test]
    fn a_declaration_takes_groups_and_connects_like_an_at_form() {
        let src = "::x(a)[ b ]{ c: 1 }#(i)::rule(allow:(k))\n";
        let root = parse_cst(src);
        assert_eq!(root.text().to_string(), src);
        let el = find(&root, K::BLOCK_FORM).unwrap();
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
        assert_eq!(count(&root, K::BLOCK_FORM), 2);
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
            assert_eq!(count(&root, K::BLOCK_FORM), 0, "{src:?}");
            assert_eq!(count(&root, K::INLINE_FORM), 0, "{src:?}");
        }
    }
}
