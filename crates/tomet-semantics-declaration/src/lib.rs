//! =[ tomet-declaration ]
//!
//! What a document's declarations mean: `::kind`, `::version`, `::meta`, `::macro`, `::var` and
//! the rest of the standard ones, apart from what body elements and vocabularies mean.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Declaration means `::declaration`)
//!   | A declaration is a `::name(args)` at the top of a document, about the document
//!   | rather than in it; only the header of `::blueprint` and of `::dictionary` add a `{data}`
//!   | body to it. This crate is that and only that: a datetime, a type or an
//!   | expression is another concept and lives elsewhere.
//! - (Singular names, a path in the head)
//!   | Every name is singular, and what a declaration holds is written either in the head as a
//!   | dot-path, `::field.author(type: string)[ the author ]`, or grouped under the name,
//!   | `::field(author: { type: string })`. [`Declaration::entries`] is the one place the two meet,
//!   | so every reader sees the same entries. A first positional argument is the entry itself
//!   | (`X.X` is `X`), and a trailing `[...]` is its `about` or, for a macro, its value. The plural
//!   | spellings (`::fields`) are retired and reported as such ([`Problem::Retired`]).
//! - (Shape here, meaning injected)
//!   | [`DeclKind`] is the closed list of the standard declarations. Each has a [`Spec`]: its
//!   | positional slots, whether it may be written once or many times, where it may be written
//!   | ([`Scope`]), and what its parentheses hold ([`Shape`]). Reading and checking the ones both hosts have (`kind version meta macro
//!   | var`) is here. Standard declarations are counted here, in one place for both hosts; the
//!   | validator's singleton check covers only user-vocabulary elements. What the `.tmt`-only
//!   | ones *do* -- `use` binds a vocabulary, `blueprint` is instantiated, `config` fills the
//!   | settings -- stays with the host, which matches on [`DeclKind`]. This crate never calls the
//!   | host back.
//! - (A leaf)
//!   | It depends on [`tomet_ast`] for [`Value`], [`Element`] and [`File`], on
//!   | [`tomet_tree`] to walk a file's top level, and on [`tomet_type`] for the types a
//!   | `::var` entry names. It does not know a body element, a vocabulary or a vault.
//! - (Two hosts, one input)
//!   | A `.tmt` document gives its declarations as [`Declaration`]s through
//!   | [`Declaration::from_ast`]; a `.ctmt` file's header gives the same three things, a
//!   | name, `(args)` and `{data}`. The readers see only that.
//!
//! =[ Not here ]
//!
//! - `::rule(...)`, a document-level rule: it exists as a name in a design memo only. `::rule`,
//!   the connect on an element, is another concept.
//! - A user vocabulary's preamble elements (`::character`): the vocabulary declares them.
//! - AST-anchored diagnostics with source spans: they live in `tomet-validator-core`, which
//!   maps [`Problem`] back to an element's span.

pub use tomet_ast::{Element, File, Value};

mod check;
mod declaration;
mod kind;
mod macros;
mod profile;
mod read;
mod vars;

pub use check::{Problem, check_declarations};
pub use declaration::{Declaration, EntriesError, Found, declarations, declarations_from_header};
pub use kind::{Cardinality, DeclKind, Host, Prose, Scope, Shape, Spec};
pub use macros::{Arity, Placeholder, expressions_in_body, is_ident_placeholder, placeholder_of};
pub use profile::Profile;
pub use read::{
    blueprint_target, dictionary_name, document_blueprint, document_dictionary, document_entries,
    document_id, document_kind, document_macros, document_meta, document_profiles, document_vars,
    document_version, id_name, kind_name, macros, meta, profile_names, version_string,
};
pub use vars::{VAR_KEYS, VarDecl, read_var_decls};
