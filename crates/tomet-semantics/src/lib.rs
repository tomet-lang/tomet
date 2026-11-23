//! =[ tomet-semantics ]
//!
//! I/O-free semantic classification and normalization layer for Tomet AST elements.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - (Pure I/O-Free Semantic Layer)
//!   | `tomet-semantics` is strictly a pure classification and semantic extraction layer.
//!   | It performs zero I/O and does not depend on `tomet-parser`, allowing downstream
//!   | consumers (`tomet-html`, `tomet-markdown`, `tomet-links`, `tomet-tui`, `tomet-lsp`) to
//!   | classify elements without pulling in the parser.
//! - (Declarations are not here)
//!   | What `::kind`, `::meta`, `::macro`, `::var` and the other standard declarations *mean* --
//!   | their slots, how often each may be written, what reading one gives -- is
//!   | `tomet-declaration`'s, so a `.ctmt` file reads them the same way. This crate keeps
//!   | what only a tomet document does with them: [`ElementKind::Declaration`] classifies one,
//!   | [`DocumentConfig`] fills the export settings from `::export`, and a vocabulary
//!   | binds through `::use`. Which elements carry an address (`@link`, `@embed`, `@file`,
//!   | `@dir`) is `tomet-address`'s.
//!
//! =[ Core Capabilities ]
//!
//! - (Element Classification (`kind`))
//!   | Canonical recognition of Tomet's built-in vocabulary
//!   | (`::meta` and the other declarations, `@link`, `@embed`, `hr`, `em`, `strong`, `mark`,
//!   | `raw`, `quote`, `table`, `heading`, `bare`, `interp`) via [`ElementKind`].
//! - (Link Target Extraction (`target`))
//!   | Canonical extraction of link targets and scheme
//!   | prefix classification (`Url`, `File`, `Tm`, `Id`, `Ref`) via [`TargetScheme`].
//! - **Positional Argument Normalization (`positional`))
//!   | Unifies positional argument mapping
//!   | (e.g. `@raw(rust)` -> `{lang: "rust"}`) for built-in and vocabulary-declared schemas.
//! - (Document Configuration (`config`))
//!   | Reads a document's `::export` into a structured [`DocumentConfig`], with the `::macro` and
//!   | `::var` it declares.
//! - (Dictionaries (`dictionary`))
//!   | Reads a `::dictionary(name)` document into a [`Dictionary`] of [`Term`]s, collects the ones a
//!   | vault registers in [`LoadedDictionaries`], and resolves what a `@term` names ([`TermRef`]).
//! - (Structural Helpers)
//!   | Heading level clamping (`heading`) and
//!   | table parsing (`table`).
//! - (Element Data Extraction (`data`))
//!   | Extracts and normalizes an element's `{...}` data group via [`data::element_data`].
//! - (Element-Specific Semantics (`elements`))
//!   | Dedicated normalization and extraction
//!   | logic for concrete built-in element families (`comment`, `config`, `footnote`,
//!   | `heading`, `table`, `tag`, `target`).
//!

mod connect_member;
pub mod data;
pub mod dictionary;
pub mod elements;
mod export_links;
pub mod flatten;
mod kind;
mod positional;
mod rebase;
pub mod vocabulary;

pub use connect_member::{
    CONNECT_MEMBERS, ConnectMember, UnknownConnectMember, classify_connect_member,
};
pub use data::element_data;
pub use dictionary::{Definitions, Dictionary, LoadedDictionaries, Term, TermProblem, TermRef};
pub use elements::comment::{comment_element, comment_text};
pub use elements::config::{DocumentConfig, ExportType, document_config};
pub use elements::footnote::{self, FootnoteItem, FootnoteRegistry};
pub use elements::heading::heading_level;
pub use elements::table::{TableCell, TableRow, parse_table_rows};
pub use elements::tag::{self, extract_tags};
pub use elements::target::{
    EXPLICIT_SCHEMES, TargetScheme, link_target, link_target_of, path_target, path_target_of,
    target_scheme,
};
pub use export_links::{ExportDestination, ExportTarget, rewrite_export_links};
pub use flatten::{
    EXACT_DATA_KEY, FlatData, POSITIONAL_KEY, flatten_data, flatten_element_data, is_attribute_key,
    value_to_json,
};
pub use kind::{
    ContentShape, ElementKind, Shape, UnknownName, builtin_content_shape, builtin_kinds,
    classify_declaration, classify_declaration_lenient, classify_std, classify_std_attribute_name,
    classify_std_lenient, classify_std_name, is_directive, is_written_in_std, required_shape,
    shape_mismatch,
};
pub use positional::{
    LIST_MARKER_POSITIONAL_KEY, builtin_positional_arg_keys, constructor_args_view,
    declaration_args_view, declaration_args_view_in, element_args_view, element_args_view_in,
    list_marker_view,
};
pub use rebase::rebase_relative_links;
pub use tomet_declaration::DeclKind;
/// The view the `*_args_view` functions return, so a caller can name it without the core crate.
pub use tomet_semantics_core::TupleView;
pub use tomet_tree::{ElementExt, ValueExt};
pub use vocabulary::{
    ATTRIBUTE_KEYS, AttributeDecl, Bindings, CONSTRUCTOR_KEYS, CallError, Constructed,
    ConstructorDecl, ContentAllow, ELEMENT_KEYS, ElementDecl, EntryProblem, HOST_VALUES,
    LoadedVocabularies, PARAM_KEYS, ParamDecl, RESERVED_ELEMENT_NAMES, RESERVED_NAMESPACES, TABLES,
    USE_ARGS, Use, UseProblem, VersionMismatch, Vocabulary, builtin_doc_vocabularies,
    check_call_params, entry_problems_in, is_alias, is_reserved_element_name, read_call,
    use_problems, uses,
};
