//! A vault's parsed documents assembled in memory into one tree: [`Vault`] > [`Folder`] >
//! File ([`NamedFile`]) > [`Section`](tomet_ast::Section).
//!
//! The four levels stay four different things. What they share is the shape of a node: a
//! heading part ([`Heading`]) and the nodes that sit directly inside it. [`children`] says what is
//! in what, [`walk()`] is the one recursion over that, and [`Node::heading`] gives the heading
//! part of any level, the way `tomet-tree` does for the nodes inside a single file.
//!
//! =[ What each level is ]
//!
//! - (Vault)
//!   | The vault's config document ([`Vault::config`]) is its heading part. Its one child is the
//!   | vault's root directory, which is a [`Folder`] like any other.
//! - (Folder)
//!   | A directory. Its entry document ([`Folder::entry`], named by the vault) is its own node:
//!   | the leading `::` declarations of it are the Folder's heading part and its sections sit
//!   | directly in the Folder, before the Folder's [`Child`]ren. The entry is not itself a child.
//!   | A directory with no entry document has an empty heading part.
//! - (File)
//!   | A parsed document ([`NamedFile`]). Its `::` declarations are its heading part and its
//!   | top-level sections are its children.
//! - (Section)
//!   | Not a type of this crate. A [`Node::Section`] borrows the section out of the AST of the
//!   | document that holds it, and its children are the sections nested in it. A section has no
//!   | declarations, so its heading part is its title, `(args)`, `{value}`, `#(id)` and
//!   | `::connect`s.
//!
//! =[ What this crate does not do ]
//!
//! - (No disk)
//!   | It is given parsed documents, each with the vault-relative path it was read from, and
//!   | does no I/O. Finding the files, deciding which of them belong to the vault and parsing them
//!   | is `tomet-load`'s.
//! - (No config types)
//!   | The name of a Folder's entry document and the [`Collator`](tomet_sort::Collator) that orders
//!   | children are arguments of [`Tome::build`]. Where they come from is the caller's.
//! - (No joining)
//!   | The tree is borrowed as it is. Putting it into one document, or giving it addresses,
//!   | is not done here.
//!
//! =[ Why it sits in its layer ]
//!
//! Pure and in memory, like `tomet-vault`: it depends on `tomet-ast` for what it assembles,
//! `tomet-tree` for the [`Visit`](tomet_tree::Visit) its walk answers to, and `tomet-sort` for
//! the order of children, and on nothing that touches a disk.

mod build;
mod model;
mod node;

pub use build::BuildError;
pub use model::{Child, Folder, NamedFile, Tome, Vault};
pub use node::{Heading, Node, children, walk};

#[cfg(test)]
mod tests;
