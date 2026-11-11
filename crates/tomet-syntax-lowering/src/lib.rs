//! =[ tomet-lowering ]
//!
//! Lowering the Tomet Concrete Syntax Tree (CST) into typed AST representations
//! ([`tomet_ast::File`], [`tomet_ast::Value`]).
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Deterministic Static Lowering Boundary)
//!   -| `tomet-lowering` is strictly a pure, side-effect-free, deterministic lowering engine.
//!    | It performs zero I/O, external file resolution, or dynamic code execution.
//!    | Given an identical CST, it produces identical AST output.
//! - (Nesting Limit)
//!   -| Past [`tomet_cst::MAX_DEPTH`] levels the CST keeps the text flat in a `TOO_DEEP` node,
//!    | and lowering refuses it with an error.
//! - (Nexus Primary Group)
//!   -| Lowering refuses a nexus (`@name`, a `::name` declaration, a `::name` connect) without a
//!    | primary group, `(args)` or `[content]`, at the nexus, before it reads any group: a
//!    | nameless `::(...)` connect would otherwise merge into the args and pass for them. A `::`
//!    | declaration takes no connect. The grammar is in the `tomet-cst-parser` crate docs.
//! - (Type Seats)
//!   -| A `<$type(args)>` is read from the text between its `<$` and `>` by the expression parser
//!    | the interpolations use, for a type is an expression, and is kept as written: a name or a
//!    | call of one, nothing else. It sits where a hole does (a block, running text, a value), and
//!    | not as an entry of a `{...}` group, which holds `key: value` entries and elements. Which
//!    | names are types, and what their arguments mean, is not asked here.
//! - (Connects)
//!   -| A connect is `::name(...)` or the nameless `::{...}` / `::(...)`. The retired single-colon
//!    | form is kept in the CST as a connect and refused here, with the spelling to write.
//!    | `::id(x)` and `::meta(a: 1)` are the `#(x)` and `{a: 1}` of their owner, written out.

pub mod element;
pub mod error;
pub mod inline;
pub mod interp;
pub mod lower;
pub mod value;

pub use error::{Error, Result};
pub use lower::{is_declaration, lower_declaration, lower_file};
pub use value::{lower_data_file, lower_value};

#[cfg(test)]
mod tests;
