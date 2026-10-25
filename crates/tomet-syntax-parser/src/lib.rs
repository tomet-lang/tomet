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
//!    | hole (`Sigil::Question`). The parser only builds the hole; what a matcher is and
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
//! - (document) Block-level dispatch loop, paragraphs, and document-level parsing.
//! - (element) Typed elements (`<T>`, `@name`, bare `@`), group ordering, and connect syntax.
//! - (section) Headings (`=[...]`) and thematic breaks (`---`, `---[ title ]---`).
//! - (list) Ordered and unordered lists (`-`, `-.`), markers, and item attributes.
//! - (raw/fence)| Fenced code blocks (``` ``` ```) and the
//!              | line-oriented fence scan they share.
//! - (inline) Inline scanning, formatting delimiters (`*em*`, `**strong**`, `==mark==`), autolinks.
//! - (interp) `${...}` interpolation expressions (identifiers, member chains, calls, literals).
//! - (value) Lightweight Tomet data grammar (maps, sequences, scalars, comments).
//! - (cst) Lossless Concrete Syntax Tree parsing powered by `tomet-cst`.
//! - (error) Parse errors: a message plus a byte offset, with line and column resolved once at the entry points (`located_in`).

pub mod cst;
mod dialect;
mod document;
mod element;
mod error;
mod fence;
mod inline;
mod interp;
mod list;
mod raw;
mod section;
mod value;

pub use cst::parse_cst;
pub use document::{parse_document, parse_pattern};
pub use error::{Error, Result};
pub use value::{CotometValueHook, parse_value};

/// Parse a `$func(args)` or `${expr}` dollar element directly from a string slice.
pub fn parse_dollar_element_str(source: &str) -> Result<tomet_ast::Element> {
    let mut cur = tomet_lexer::Cursor::new(source);
    interp::parse_dollar_element(&mut cur).map_err(|e| e.located_in(source))
}

#[cfg(test)]
mod tests;
