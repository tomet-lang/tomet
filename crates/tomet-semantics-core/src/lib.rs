//! =[ tomet-semantics-core ]
//!
//! Shared core semantic model and type admission for Tomet and cotomet documents: whether a value is
//! of a type ([`admits()`]), what a type's written `default` is as a value ([`default_value`]) and
//! whether it is of the type that carries it ([`admits_default`]), and which values are not known
//! until they are evaluated or filled ([`is_placeholder`]).

pub mod admits;
pub mod parse;
pub mod positional;

pub use admits::{Mismatch, admits, admits_default, datum_value, default_value, is_placeholder};
pub use parse::parse_type_spec;
pub use positional::{POSITIONAL_ENTRY_KEY, bind_positional};
