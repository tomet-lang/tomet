//! =[ cotomet ]
//!
//! The data language dual to tomet (`tmt` / `ctmt`): a lightweight, expressive
//! syntax for values, maps and sequences. This is the crate to depend on when
//! using cotomet from Rust. It holds no logic of its own.
//!
//! =[ Overview ]
//!
//! - [`syntax`] is the language: parser, printer and the spelling helpers
//!   (`cotomet-syntax`). Read its crate doc for what cotomet accepts.
//! - The top level is the serde side (`serde_cotomet`): [`from_str`] and
//!   [`to_string`] read and write Rust types as cotomet.
//!
//! A crate that sits below this one in the workspace layering (the tomet
//! parser, the printers) depends on `cotomet-syntax` directly.

pub use cotomet_syntax as syntax;
pub use serde_cotomet::{
    Error, Result, Value, from_str, from_value, print_value, to_string, to_value,
};
