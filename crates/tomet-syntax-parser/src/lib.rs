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
//!    | Given identical input text, it produces identical AST output with guaranteed
//!    | linear/predictable time complexity.
//! - (Documents And Patterns)
//!   -| `parse_document` reads an ordinary document, where `?` is text.
//!    | `parse_pattern` reads the same grammar with one more sigil, `?`, which makes a
//!    | hole (see [`tomet_ast::Hole`]). The parser only builds the hole; what a matcher is and
//!    | what a hole matches belongs to the caller (tandr), never to this crate.
//! - (Tree-sitter Synchronization)
//!   -| `tomet-parser` is the single source of truth for grammar definitions.
//!    | `tree-sitter-tomet` is a separate hand-maintained grammar approximation used for
//!    | editor syntax highlighting.
//!
//! =[ Module Layout ]
//!
//! - (dialect) Which sigils may open an element: `@` always, `?` (a hole) only in a pattern.
//!   `parse_document` and `parse_pattern` share every other function; each takes the dialect.
//! - (cst) The one reader of the grammar's shape: a lossless, resilient Concrete Syntax Tree
//!   (blocks, sections, lists, elements and their groups, fences, `|` runs), powered by `tomet-cst`.
//!   A blank line or a line-start `;` breaks a block here.
//! - (lower) The CST read as an AST: `parse_document` and `parse_pattern` are `parse_cst` and
//!   then this. It builds the typed tree and refuses what a document may not say.
//! - (element) What makes `@` start an element, which characters open a group, and the readers
//!   of a group's values: `(args)`, `{value}`, `#(id)`.
//! - (value) The Tomet data grammar (maps, sequences, scalars, comments), from `cotomet-syntax`,
//!   with a hook that finds what a value embeds -- an element, `[content]`, a `|` run -- in the CST.
//! - (inline) What the CST and lowering share about running text: `Text`/`SoftBreak` items, the
//!   probes for formatting delimiters (`*em*`, `**strong**`, `==mark==`) and autolinks.
//! - (interp) `${...}` interpolation expressions (identifiers, member chains, calls, literals).
//! - (error) Parse errors: a message plus a byte offset, with line and column resolved once at the entry points (`located_in`).

pub mod cst;
mod dialect;
mod element;
mod error;
mod inline;
mod interp;
mod lower;
mod value;

pub use cst::{parse_cst, parse_cst_pattern};
pub use error::{Error, Result};
pub use value::{CotometValueHook, parse_value};

/// Parse an entire source string as a markup [`tomet_ast::Document`]: the CST of
/// the text ([`parse_cst`]), read as the AST it stands for.
pub fn parse_document(src: &str) -> Result<tomet_ast::Document> {
    lower::lower_document(src, &parse_cst(src))
}

/// Parse an entire source string as a pattern: a [`tomet_ast::Document`] in which
/// `?name(args){options}[content]` is a hole ([`tomet_ast::Hole`]).
///
/// It is [`parse_document`] with one more sigil. Everything else is read by the same code, so a
/// pattern is written in, and printed as, ordinary tomet.
pub fn parse_pattern(src: &str) -> Result<tomet_ast::Document> {
    lower::lower_pattern(src, &parse_cst_pattern(src))
}

/// Parse a `$func(args)` or `${expr}` interpolation directly from a string slice.
pub fn parse_interpolation_str(source: &str) -> Result<tomet_ast::Interpolation> {
    let mut cur = tomet_lexer::Cursor::new(source);
    interp::parse_interpolation(&mut cur).map_err(|e| e.located_in(source))
}

#[cfg(test)]
mod tests;
