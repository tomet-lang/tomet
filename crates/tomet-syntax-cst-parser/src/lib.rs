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
//!
//! =[ Nexus Grammar ]
//!
//! A nexus is what a name stands on: an element `@name`, a declaration `::name`, or a named
//! connect `:name`. A nexus stands on a primary group, and is nothing without one.
//!
//! - (Primary Group)
//!   -| `(args)` (`ARGS`) or `[content]` (`CONTENT`). A `|` run is `[content]` spelled without
//!    | brackets, so `@table` with a `|` run on the line after it has its primary group.
//! - (Secondary Groups)
//!   -| `{value}` (`VALUE_DATA`), `#(id)` (`ID_GROUP`) and a `:connect` (`CONNECT`) only add to
//!    | a nexus, and none of them, nor all of them together, can stand alone: `@name{}`,
//!    | `@name#(id)`, `@name:rule(a)` and `::name{}` are refused. A nameless connect (`:{...}`,
//!    | `:(...)`) merges into its owner's groups after the fact, so it is no primary group
//!    | either: `@name:(a)` is refused, and `@name(a):{b: 1}` is valid. A `::` declaration
//!    | takes no connect of any kind.
//! - (Where It Is Decided)
//!   -| The tree never fails, so it keeps a nexus without a primary group as a form that has none
//!    | (a `BLOCK_FORM` or `INLINE_FORM` whose children are a `SIGIL` and, at most, secondary
//!    | groups) and `tomet-lowering` refuses it, at the nexus. What is not a nexus at all is not a
//!    | form in the tree.
//! - (Bare Names Are Text)
//!   -| `@name` with nothing attached is running text (`ping @alice`, `foo@bar.com`), and so is a
//!    | `:name` with no group after it (`@alice:bob`): a `:name` is a connect only with a group
//!    | to take. A bare `@name` that stands alone on a line is a block form, to be refused --
//!    | unless the next line folds into it as running text goes on across lines, or opens a
//!    | group the name takes (a `(`, `[`, `{`, `#(`, a connect, or a `|` run). In the combine
//!    | notation `-@name`, the `-` against the `@` says the item is the element, so there it is
//!    | a form to refuse.
//! - (Not Nexuses)
//!   -| Headings and list items (`= heading {a: 1}`, `==#(id)`, `- #(id)`, `- {a: 1}`) are not
//!    | nexuses and need no primary group. Their nameless connects (`- item :{a: 1}`,
//!    | `= heading :(a)`) stay valid. Their named connects are nexuses, and are claimed the same
//!    | way on a full-form head (`-#(id) :rule{}`) and at the end of a bracket-less body
//!    | (`- item :rule{}`): wherever it is structurally one, it is kept for lowering to refuse
//!    | for want of a primary group. Structurally means `:name` and its group with no white
//!    | space between, and a group that reads: `- Note: {x}`, `- note: a:b` and
//!    | `- foo:bar{not a map}` are text.

pub mod cst;
pub mod inline;
pub(crate) mod tail;

pub use cst::{MAX_DEPTH, parse_cst, parse_cst_value};

#[cfg(test)]
mod tests;
