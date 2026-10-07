//! =[ tomet-parser ]
//!
//! The recursive-descent parser (`&str -> Document / Value`) and authoritative
//! source of truth for the Tomet grammar.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - Deterministic Static Parser Boundary:
//!   `tomet-parser` is strictly a pure, side-effect-free, deterministic static parser.
//!   It performs zero I/O, external file resolution, or dynamic code execution.
//!   Given identical input text, it produces identical AST output with guaranteed
//!   linear/predictable time complexity.
//!
//! - Tree-sitter Synchronization:
//!   `tomet-parser` is the single source of truth for grammar definitions.
//!   `tree-sitter-tomet` is a separate hand-maintained grammar approximation used for
//!   editor syntax highlighting.
//!
//! =[ Module Layout ]
//!
//! - `document`: Block-level dispatch loop, paragraphs, and document-level parsing.
//! - `element`: Typed elements (`<T>`, `@name`, bare `@`), group ordering, and connect syntax.
//! - `section`: Headings (`=[...]`) and thematic breaks (`---`, `---[ title ]---`).
//! - `list`: Ordered and unordered lists (`-`, `-.`), markers, and item attributes.
//! - `codeblock` / `fence`: Fenced code blocks and raw verbatim fence spans (`+++`).
//! - `inline`: Inline scanning, formatting delimiters (`*em*`, `**strong**`, `==mark==`), autolinks.
//! - `interp`: `${...}` interpolation expressions (identifiers, member chains, calls, literals).
//! - `caret`: `^(id)` or `^name(id)` caret reference element parsing.
//! - `value`: Lightweight Tomet data grammar (maps, sequences, scalars, comments).
//! - `cst`: Lossless Concrete Syntax Tree parsing powered by `tomet-cst`.
//! - `error`: Source location (`Position`/`Span`) aware error diagnostics.

mod caret;
mod codeblock;
pub mod cst;
mod document;
mod element;
mod error;
mod fence;
mod inline;
mod interp;
mod list;
mod section;
mod value;

pub use cst::parse_cst;
pub use document::parse_document;
pub use error::{Error, Result};
pub use value::parse_value;

/// Parse a `$func(args)` or `${expr}` dollar element directly from a string slice.
pub fn parse_dollar_element_str(source: &str) -> Result<tomet_ast::Element> {
    let mut cur = tomet_lexer::Cursor::new(source);
    interp::parse_dollar_element(&mut cur)
}

/// Parse a `^(id)` or `^name(id)` caret reference element directly from a string slice.
pub fn parse_caret_element_str(source: &str) -> Result<tomet_ast::Element> {
    let mut cur = tomet_lexer::Cursor::new(source);
    caret::parse_caret_element(&mut cur, true)
}

#[cfg(test)]
mod tests;
