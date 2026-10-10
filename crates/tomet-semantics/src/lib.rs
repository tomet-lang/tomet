//! =[ tomet-semantics ]
//!
//! I/O-free semantic classification and normalization layer for Tomet AST elements.
//!
//! =[ Architecture & Responsibilities ]
//!
//! - Pure I/O-Free Semantic Layer:
//!   `tomet-semantics` is strictly a pure classification and semantic extraction layer.
//!   It performs zero I/O and depends exclusively on `tomet-ast` (not `tomet-parser`),
//!   allowing downstream consumers (`tomet-html`, `tomet-markdown`, `tomet-links`,
//!   `tomet-tui`, `tomet-lsp`) to classify elements without pulling in the parser.
//!
//! =[ Core Capabilities ]
//!
//! - **Element Classification (`kind`)**: Canonical recognition of Tomet's built-in vocabulary
//!   (`@meta`, `@config`, `@links`, `@link`, `<embed>`, `hr`, `em`, `strong`, `mark`,
//!   `raw`, `quote`, `table`, `heading`, `ol`, `ul`, `bare`, `interp`) via [`ElementKind`].
//! - **Link Target Extraction (`target`)**: Canonical extraction of link targets and scheme
//!   prefix classification (`Url`, `File`, `Tm`, `Id`, `Ref`) via [`TargetScheme`].
//! - **Positional Argument Normalization (`positional`)**: Unifies positional argument mapping
//!   (e.g. `@raw(rust)` -> `{lang: "rust"}`) for built-in and `@settings` custom schemas.
//! - **Document Configuration (`config`)**: Merges `@config` and `@settings` blocks across
//!   a document into a structured [`DocumentConfig`].
//! - **Metadata Extraction (`meta`)**: Extracts top-level `@meta` values via [`document_meta`].
//! - **Structural Helpers**: Heading level clamping (`heading`) and
//!   table parsing (`table`).
//! - **Element-Specific Semantics ([`elements`])**: Dedicated normalization and extraction
//!   logic for concrete built-in element families (`config`, `embedded`, `footnote`,
//!   `heading`, `meta`, `table`, `tag`, `target`).
//!

mod connect_member;
pub mod elements;
pub mod flatten;
mod kind;
mod normalize;
mod positional;
pub mod vocabulary;

pub use connect_member::{
    CONNECT_MEMBERS, ConnectMember, UnknownConnectMember, classify_connect_member,
};
pub use elements::config::{DocumentConfig, ExportType, document_config};
pub use elements::embedded;
pub use elements::footnote::{self, FootnoteItem, FootnoteRegistry};
pub use elements::heading::heading_level;
pub use elements::meta::{document_kind, document_meta, document_version};
pub use elements::table::{TableCell, TableRow, parse_table_rows};
pub use elements::tag::{self, extract_tags};
pub use elements::target::{
    EXPLICIT_SCHEMES, TargetScheme, link_target, link_target_of, path_target, path_target_of,
    target_scheme,
};
pub use flatten::{
    EXACT_DATA_KEY, FlatData, POSITIONAL_KEY, flatten_data, flatten_element_data, scalar_string,
    value_to_json,
};
pub use kind::{
    BUILTIN_KINDS, ContentShape, ElementKind, Shape, UnknownName, builtin_content_shape,
    builtin_region, builtin_singleton, classify_std, classify_std_lenient, classify_std_name,
    is_directive, required_shape, shape_mismatch,
};
pub use normalize::normalize_data_value;
pub use positional::{
    LIST_MARKER_POSITIONAL_KEY, builtin_positional_arg_keys, normalized_element_args,
    normalized_element_args_in, normalized_list_marker,
};
pub use tomet_tree::{ElementExt, ValueExt};
pub use vocabulary::{
    Bindings, ContentAllow, ElementDecl, LoadedVocabularies, ParamDecl, RESERVED_NAMESPACES,
    Region, Vocabulary, builtin_doc_vocabularies,
};
