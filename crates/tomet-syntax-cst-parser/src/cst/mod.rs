//! Resilient, lossless Concrete Syntax Tree (CST) parser for Tomet.
//!
//! Preserves 100% of source characters including trivia (whitespace, newlines, comments).
//!
//! This is the one reader of the grammar's shape -- where a block, a form, a
//! group starts and ends. `parse_document` is this tree lowered into the AST
//! (by the `tomet-lowering` crate), so a grammar change is made here once. The parser itself
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
//! `{$ expr }` and `$(ref)` are each one `INTERP_EXPR`, the whole form, in an `INLINE_FORM`; `$name(args)`
//! is a `SIGIL` and its `ARGS`. Inside `ARGS`/`VALUE_DATA` an entry is a
//! `MAP_ENTRY` (`key: value`) or a `SEQ_ITEM`, and nested groups are
//! `MAP_LITERAL`, `CONTENT` (a value written `[ ... ]` is blocks, as in the AST
//! parser; there is no list literal) or `ARGS` -- a bare `(a, b)` is a sequence,
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
//! decided by the same functions, so the two cannot drift.
//! The tokenizer is context-free and emits tokens finely enough that delimiters
//! and structural boundaries are already token boundaries. The parser never cuts
//! tokens; it assembles compound structures (such as string literals in value
//! contexts and compound identifiers) where the grammar requires them.
//!
//! A `|` run is `[content]` spelled without brackets, and so it is the same
//! node: a `CONTENT` that opens with a `PIPE` and has no closing `]`. It is a
//! form's content (`@quote| text`, or `|` on the line after the form), a
//! list item's (`-| text`), a heading's (`=| text`), a titled thematic break's (`|` lines between
//! two `---` lines, see `parse_thematic_break`) or a `key: | prose` value.
//! It ends at the first line that does not repeat its marker in the same column
//! -- a blank line, or a `|` somewhere else, ends it too -- and the error for that
//! last case (a marker in a column that continues nothing) is lowering's, not the tree's. The `|` that begins each later
//! line is a `PIPE` token among the content's own, but the grammar reads it as
//! white space, so the blocks of a run are the blocks the bracketed form has.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use tomet_cst::{GreenNodeBuilder, SyntaxKind, SyntaxNode};
use tomet_lexer::tokenize;

use crate::inline::DelimiterIndex;
use sugar_tail::TailVerdict;

use SyntaxKind as K;

mod block;
mod code_block;
mod depth;
mod form;
mod group;
mod list;
mod name;
mod pipe_run;
mod prose;
mod section;
mod slot;
mod sugar_tail;
mod tail_validity;
mod thematic_break;
mod tokens;
mod value;

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
/// reads the tree, goes deeper than this (a deeper document would abort the process with a stack
/// overflow).
pub const MAX_DEPTH: usize = tomet_cst::MAX_DEPTH;

/// The parser state. Its methods live next to the construct they read, one file each (see the
/// `//!` of every file module); the `tokens` module holds the ones all of them share.
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
}
