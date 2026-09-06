//! I/O-free classification of what a parsed `Element` officially means --
//! recognizing Tomet's own built-in vocabulary (`@meta`, `@config`,
//! `@links`, `@link`, ...) by name only (no inference from `args`) -- plus
//! small `Document`-level lookups (`document_meta`) built directly on top
//! of that classification. Depends only on `tomet-ast`; consumers
//! (`tomet-html`, `tomet-markdown`, the CLI/TUI) use this instead
//! of each carrying their own copy of this logic.

mod config;
mod connect;
pub mod embedded;
pub mod flatten;
mod heading;
mod kind;
mod list;
mod meta;
mod positional;
mod table;
mod target;
pub mod vocabulary;

pub use config::{DocumentConfig, ExportType, document_config};
pub use connect::merge_connected_values;
pub use flatten::{
    EXACT_DATA_KEY, FlatData, POSITIONAL_KEY, flatten_data, flatten_element_data, scalar_string,
    value_to_json,
};
pub use heading::heading_level;
pub use kind::{
    BUILTIN_KINDS, ElementKind, Shape, UnknownName, builtin_region, builtin_singleton,
    classify_std, classify_std_lenient, classify_std_name, is_directive, required_shape,
    shape_mismatch,
};
pub use list::{list_items, list_ordered};
pub use meta::{document_kind, document_meta, document_version};
pub use positional::{
    LIST_MARKER_POSITIONAL_KEY, builtin_positional_arg_keys, normalized_element_args,
    normalized_element_args_in, normalized_list_marker,
};
pub use table::{TableCell, TableRow, parse_table_rows};
pub use target::{TargetScheme, link_target, link_target_of, target_scheme};
pub use tomet_tree::{ElementExt, ValueExt};
pub use vocabulary::{Bindings, ElementDecl, ParamDecl, Region, Vocabulary};
