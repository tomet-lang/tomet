//! I/O-free classification of what a parsed `Element` officially means --
//! recognizing Tomet's own built-in vocabulary (`@meta`, `@config`,
//! `@links`, `@link`, ...) by name only (no inference from `args`) -- plus
//! small `Document`-level lookups (`document_meta`) built directly on top
//! of that classification. Depends only on `tomet-ast`; consumers
//! (`tomet-html`, `tomet-markdown`, the CLI/TUI) use this instead
//! of each carrying their own copy of this logic. See
//! `docs/develop/architecture.md` for how this crate fits into the rest of
//! the pipeline.

mod config;
mod connect;
pub mod embedded;
mod heading;
mod kind;
mod list;
mod meta;
mod positional;
mod table;
mod target;

pub use config::{DocumentConfig, ExportType, document_config};
pub use connect::merge_connected_values;
pub use heading::heading_level;
pub use kind::{
    BUILTIN_KINDS, ElementKind, Shape, UnknownName, classify, classify_lenient, classify_name,
    shape_mismatch,
};
pub use list::{list_items, list_ordered};
pub use meta::{document_kind, document_meta, document_version};
pub use positional::{
    ElementSchema, LIST_MARKER_POSITIONAL_KEY, SettingsSchema, builtin_positional_arg_keys,
    normalized_element_args, normalized_element_args_with_schema, normalized_list_marker,
};
pub use table::{TableCell, TableRow, parse_table_rows};
pub use target::{TargetScheme, link_target, link_target_of, target_scheme};
pub use tomet_tree::{ElementExt, ValueExt};
