//! =[ tomet-cst-parser ]
//!
//! The lossless, resilient Concrete Syntax Tree parser for Tomet.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Deterministic Static Parser Boundary)
//!   -| `tomet-cst-parser` is strictly a pure, side-effect-free, deterministic static parser.
//!    | It performs zero I/O, external file resolution, or dynamic code execution.
//!    | Given identical input text, it produces an identical lossless CST output.
//! - (Nesting Limit)
//!   -| Past [`MAX_DEPTH`] levels the CST keeps the text flat in a `TOO_DEEP` node, still
//!    | lossless.
//! - (Zero AST Dependency)
//!   -| `tomet-cst-parser` knows only the concrete syntax tree and lexical structure,
//!    | having zero dependency on AST representations or semantic lowering.

pub mod cst;
pub mod inline;
pub(crate) mod tail;

pub use cst::{MAX_DEPTH, parse_cst, parse_cst_value};

#[cfg(test)]
mod tests;
