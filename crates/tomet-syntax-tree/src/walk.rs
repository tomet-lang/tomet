//! One definition of what sits inside what in a [`File`](tomet_ast::File), and the walkers built on it.
//!
//! [`children`] says, for any [`Node`], which nodes sit directly inside it and in which [`Place`].
//! [`walk`] is the one recursion over that, through [`walk_node_scoped`], which also says when a
//! node is done with, for a walker that must know where a block ends. Every other walker here is [`walk`] with a filter
//! and, where a position must stay unseen, a [`Visit::Skip`] written at the walker, so what a
//! walker leaves out is read in its own body and not in a copy of the recursion.
//!
//! Traversal must never be reimplemented by handwritten AST recursion across downstream crates.
//! All structural walks across the workspace route through [`children`] and [`walk`].
//!
//! The groups of a node come in one order: `(args)`, `{value}`, `[content]`, then its
//! `::connect`s. The AST keeps no order of groups, so this is the order of every walker.
//!
//! The one exception to "one recursion" is the mutable walk ([`for_each_element_mut`],
//! [`for_each_id_mut`], [`fill_interpolations`], [`fill_type_seats`], [`fill_type_seats_in_file`]): a mutable walk cannot share the borrow with
//! [`children`], so it lists the children again, and a test holds the two to the
//! same reach.

mod each;
mod fill;
mod mutate;
mod node;
#[cfg(test)]
mod tests;

pub use each::*;
pub use fill::*;
pub use mutate::*;
pub use node::*;
