//! =[ tomet-compute ]
//!
//! Expression evaluation for the `${...}` expressions of a Tomet document
//! ([`InterpExpr`]), following `tmtroot/spec/expressions/`.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Pure Compute Boundary (Zero I/O))
//!   | Evaluation performs no I/O and reads no clock. What it cannot know by
//!   | itself -- another document, a directory, the vault's settings, the
//!   | time -- comes through [`eval::Resolver`] and [`eval::Generate`], which the
//!   | layers above implement.
//! - (Typed values)
//!   | [`eval::EvalValue`] is not the `.tmt` data model ([`tomet_ast::Value`]): it
//!   | also holds documents, directories, functions and datetimes, which can
//!   | be the result of an expression and never data. A fragment (`link`,
//!   | `include`) is the one non-scalar value that can be placed in output.
//! - (Names by position)
//!   | A name is looked up in the one table its position selects: a call in
//!   | the builtin functions, a bare name in the lambda parameters and then
//!   | the reserved values ([`eval::RESERVED_VALUES`]). There is no fall
//!   | through, and an unknown name is an error.
//! - (No conversions)
//!   | Operators and methods use the values they are given and convert none
//!   | of them. `&&` and `||` skip their right side once the left decides,
//!   | and take booleans only.
//! - (When it runs)
//!   | [`eval::Phase::Output`] is rendering; the same expression gives the
//!   | same result every time. [`eval::Phase::Create`] is making a document
//!   | from a blueprint, and is the only place `generate.*` and `vars` exist.
//!
//! The pass that puts the values back into a document is
//! `tomet_transform::resolve_expressions`.

pub mod eval;
