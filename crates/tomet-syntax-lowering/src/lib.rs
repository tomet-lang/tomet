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

pub mod element;
pub mod error;
pub mod inline;
pub mod interp;
pub mod lower;
pub mod value;

pub use error::{Error, Result};
pub use lower::{is_declaration, lower_declaration, lower_file};
pub use value::{lower_data_file, lower_value};
