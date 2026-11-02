//! =[ tomet-semantics-core ]
//!
//! Shared core semantic model and type admission for Tomet and cotomet documents.

pub mod admits;
pub mod parse;
pub mod positional;

pub use admits::{Mismatch, admits};
pub use parse::parse_type_spec;
pub use positional::{POSITIONAL_ENTRY_KEY, bind_positional};
