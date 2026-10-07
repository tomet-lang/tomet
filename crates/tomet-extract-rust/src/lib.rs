//! Extraction of Tomet API documentation from Rust crates using `syn`.
//!
//! This crate parses Rust source files with `syn`, traverses crate and module
//! hierarchies, inspects item visibilities (`pub`), and extracts documentation
//! comments (`//!` and `///`).
//!
//! Because doc comments are assumed to be written in Tomet markup syntax,
//! extracted comments are parsed into Tomet AST [`tomet_ast::Document`] blocks,
//! creating a unified `.tmt` document for each Rust crate.

pub mod extract;

pub use extract::{RustCrateSource, RustExtractOptions, extract_crate_doc, extract_file_doc};
