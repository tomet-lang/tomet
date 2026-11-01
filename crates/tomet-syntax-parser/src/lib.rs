//! =[ tomet-parser ]
//!
//! The recursive-descent parser (`&str -> Document / Value`) and authoritative
//! source of truth for the Tomet grammar.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Deterministic Static Parser Boundary)
//!   -| `tomet-parser` is strictly a pure, side-effect-free, deterministic static parser.
//!    | It performs zero I/O, external file resolution, or dynamic code execution.
//!    | Given identical input text, it produces identical AST output, in time proportional to the
//!    | input: no construct is decided by trying it and reading the rest of the line again (the
//!    | `tail` module below is why), and none nests deeper than [`cst::MAX_DEPTH`].
//! - (Nesting Limit)
//!   -| Past [`cst::MAX_DEPTH`] levels the CST keeps the text flat in a `TOO_DEEP` node, still
//!    | lossless, and lowering refuses it with an error. A deeper document used to abort the
//!    | process with a stack overflow, so no recursion here, or in what reads the tree, goes deeper.
//! - (Boxes)
//!   -| `<...>` is one family in the syntax: a name tag `<name>` and a hole `<?matcher(args)>`.
//!    | `parse_document` reads both with no mode. A hole becomes a [`tomet_ast::Hole`]; a name tag
//!    | stays text, since tomet gives it no meaning. The parser only builds the hole; what a matcher
//!    | is and what a hole matches belongs to the caller (tandr), never to this crate.
//! - (Tree-sitter Synchronization)
//!   -| `tomet-parser` is the single source of truth for grammar definitions.
//!    | `tree-sitter-tomet` is a separate hand-maintained grammar approximation used for
//!    | editor syntax highlighting.
//!
//! =[ Module Layout ]
//!
//! - (cst) The one reader of the grammar's shape: a lossless, resilient Concrete Syntax Tree
//!   (blocks, sections, lists, elements and their groups, `<...>` boxes, fences, `|` runs), powered by `tomet-cst`.
//!   A blank line or a line-start `;` breaks a block here.
//! - (lower) The CST read as an AST: `parse_document` is `parse_cst` and then this. It builds the typed tree and refuses what a document may not say.
//! - (element) The readers of a group's values: `(args)`, `{value}`, `#(id)`. Where an element or a
//!   hole starts inside a group is the CST's to say.
//! - (value) The Tomet data grammar (maps, sequences, scalars, comments), from `cotomet-syntax`,
//!   with a hook that finds what a value embeds -- an element, `[content]`, a `|` run -- in the CST.
//! - (tail) Where a bracket-less body's trailing groups (`- text {value}`, `= text #(id)`) can be,
//!   from the characters alone; the CST asks the grammar once about what that finds.
//! - (inline) What the CST and lowering share about running text: `Text`/`SoftBreak` items, the
//!   probes for formatting delimiters (`*em*`, `**strong**`, `==mark==`) and autolinks.
//! - (interp) `${...}` interpolation expressions (identifiers, member chains, calls, literals).
//! - (error) Parse errors: a message plus a byte offset, with line and column resolved once at the entry points (`located_in`).

pub mod cst;
mod element;
mod error;
mod inline;
mod interp;
mod lower;
mod tail;
mod value;

pub use cst::parse_cst;
pub use error::{Error, Result};
pub use value::{CotometValueHook, parse_value};

/// Parse an entire source string as a markup [`tomet_ast::Document`]: the CST of
/// the text ([`parse_cst`]), read as the AST it stands for.
pub fn parse_document(src: &str) -> Result<tomet_ast::Document> {
    lower::lower_document(src, &parse_cst(src))
}

/// Parse a `$func(args)` or `${expr}` interpolation directly from a string slice.
pub fn parse_interpolation_str(source: &str) -> Result<tomet_ast::Interpolation> {
    let mut cur = tomet_lexer::Cursor::new(source);
    interp::parse_interpolation(&mut cur).map_err(|e| e.located_in(source))
}

#[cfg(test)]
mod tests;
