//! =[ tomet-type ]
//!
//! The types a value can be declared as, the arguments they take, and how text
//! is held to them.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (One vocabulary)
//!   | A `type:` is written in the settings' `meta` fields, in `::var`, and in
//!   | a vocabulary's arguments. They all name a [`Type`] here, so a name that
//!   | one place accepts is a name every place accepts.
//! - (A pure base-layer crate)
//!   | This crate is in the base layer (`crates/tomet-base-*`) and depends on
//!   | nothing in the workspace. A type does not know what an element is, what
//!   | an AST node is, or which vocabulary declared it.
//! - (A type carries its arguments)
//!   | A [`Type`] is a [`Kind`] and the arguments that kind takes: a `nanoid`
//!   | has a `length` and a `prefix`, a `list` an item type and a `length`,
//!   | every type a `default`. What a kind takes is part of the kind, so a
//!   | combination that makes no sense (a `string` with a `prefix`) cannot be
//!   | held.
//! - (Built from structure, not sliced from text)
//!   | [`Type::build`] is the one constructor. It takes a name and the
//!   | arguments as [`Arg`]s -- whatever read them (a document's data, the
//!   | settings, an expression) has turned them into those -- and holds them
//!   | to every rule: which arguments a kind takes, how many, of what sort,
//!   | which kinds admit `default: auto`. The text form (`list(string, length: 6)`)
//!   | is a small parser over the same constructor and the [`Display`](std::fmt::Display)
//!   | that writes it back; there is no other reader of it.
//! - (`auto` is a fact about a kind, not a draw)
//!   | `default: auto` is allowed for the kinds that can make a value up --
//!   | an id (`uuid`, `nanoid`) or the moment (`datetime`, `date`), see
//!   | [`Kind::auto`] and [`AutoFill`] -- and refused for the rest. This crate
//!   | says whether it applies and of which sort; it holds no random source
//!   | and no clock, so whoever fills the value in owns both.
//! - (What is held to a type)
//!   | [`Type::holds_text`] asks whether a string is in the form the type gives
//!   | text, for the types that have one.

mod arg;
mod build;
pub mod datetime;
mod error;
mod kind;
mod text;
mod ty;
mod version;

#[cfg(test)]
mod tests;

pub use arg::{Arg, Datum, DefaultValue};
pub use error::TypeError;
pub use kind::{AutoFill, Kind};
pub use ty::Type;
pub use version::{Identifier, Version};

/// The 8-4-4-4-12 hexadecimal form, in either case. The version and variant are
/// not looked at: what a `uuid` field asks for is an identifier of this shape,
/// not a particular way of drawing one.
pub fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}
