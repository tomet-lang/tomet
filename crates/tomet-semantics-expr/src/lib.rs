//! =[ tomet-expr ]
//!
//! Semantic model, types, variable scopes, and static outside-reach analysis
//! for the `{$ ... }` expressions of a Tomet document ([`tomet_ast::InterpExpr`]),
//! following `tmtroot/spec/expressions/`.
//!
//! Expression evaluation itself lives in `tomet-eval` (Layer 6).

mod context;
mod datetime;
mod error;
mod memory;
mod outside;
mod types;
mod value;
mod vars;

pub use context::{CacheRequest, EvalContext, NoResolver, Note, Notes, PreparedBlocks, Resolver};
pub use datetime::{datetime_member, format_datetime, parse_datetime, parse_offset};
pub use error::EvalError;
pub use memory::MemoryResolver;
pub use outside::{OwnNames, Reach, Reaching, reaches_outside};
pub use types::{
    construct_with, evaluate_type, evaluate_type_with, is_constructor, named as named_type,
    type_names,
};
pub use value::{Closure, DirRef, DocRef, Env, EvalValue, Namespace, Part};
pub use vars::{VarError, VarScope, shaped, typed_value, var_scope};

/// The names that stand for a value and that no lambda parameter may take.
pub const RESERVED_VALUES: [&str; 5] = ["self", "macro", "var", "vault", "cache"];

/// The functions an expression can call by a bare name, each with a line
/// saying what it does -- what an editor offers when `{$` is typed.
///
/// `addr` is read by the parser rather than called, and is listed because it
/// is written like a call. Every other name is one `tomet-eval` answers.
///
/// A name listed here is a function even where a type has the same name:
/// `version("1.10")` is a version and not a call of the type `version`.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("tmt", "A document by its path `tmt(\"path\")`"),
    ("ref", "A document by its note name `ref(\"name\")`"),
    (
        "addr",
        "An address written as text `addr(tmt:docs/a.tmt#intro)`",
    ),
    (
        "include",
        "A document's content, put here `include(tmt(\"path\"))`",
    ),
    ("link", "A link to a document or an element `link(target)`"),
    (
        "raw",
        "A raw code block from text `raw(text, lang: \"...\")`",
    ),
    ("add", "Arithmetic addition function `add(a, b)`"),
    ("sub", "Arithmetic subtraction function `sub(a, b)`"),
    ("mul", "Arithmetic multiplication function `mul(a, b)`"),
    ("div", "Floating-point division function `div(a, b)`"),
    ("mod", "Integer modulo function `mod(a, b)`"),
    ("not", "Negation `not(bool)`"),
    ("exists", "Whether a value is not none `exists(value)`"),
    ("emoji", "An emoji by name `emoji(\"sparkles\")`"),
    ("unicode", "A character by code point `unicode(0x2713)`"),
    (
        "version",
        "A version parsed from text `version(\"1.10.0\")`",
    ),
];

/// The names an expression can start with that are values, not functions,
/// each with a line saying what it is.
pub const RESERVED_NAMES: &[(&str, &str)] = &[
    ("self", "The document this expression is written in"),
    (
        "macro",
        "The macros defined by `::macro`: `macro.name(...)`",
    ),
    (
        "var",
        "The variables declared by `::var`: `var.name` (`tomet new --var` fills them in)",
    ),
    ("vault", "The vault's root directory: `vault::docs`"),
    (
        "cache",
        "External cached values declared outside tomet: `cache.kind(\"key\")`",
    ),
];

/// Whether `name` is a function called by a bare name, as [`FUNCTIONS`] lists them.
pub fn is_function(name: &str) -> bool {
    FUNCTIONS.iter().any(|(function, _)| *function == name)
}
